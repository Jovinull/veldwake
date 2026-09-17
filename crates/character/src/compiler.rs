//! Descriptor in, immutable character out.
//!
//! ```text
//! CharacterDescriptor -> validate() -> ValidatedDescriptor
//!                                          |
//!                              CharacterCompiler::compile
//!                                          |
//!                                   CompiledCharacter
//!                                     |-- geometry (sixteen rigid meshes)
//!                                     |-- materials (one resolved palette)
//!                                     |-- skeleton and rest pose
//!                                     |-- collision representation
//!                                     '-- gait parameters
//! ```
//!
//! A [`CompiledCharacter`] is **identity**: it is a pure function of the
//! descriptor, the compiler version, and the style-contract version, and it
//! never changes afterwards. Everything that moves — position, facing, speed,
//! locomotion phase, the result of terrain contact — is runtime state and
//! lives in `pose`, not here.
//!
//! The fingerprints below are character identity keys. They are deliberately
//! not connected to the streaming chunk cache: a character is not a chunk, and
//! a character rule must never invalidate terrain a player already has.

use veldwake_voxel::Mesh;

use crate::collision::CollisionRepresentation;
use crate::descriptor::{
    BodyMetrics, CharacterDescriptor, CharacterDescriptorError, CharacterIdentity,
    ValidatedDescriptor,
};
use crate::geometry::{GeometryError, PartScratch, PartVolume, part_volumes, validate_volumes};
use crate::hash::{fnv1a64, push_f32, push_i32, push_u32, push_u64};
use crate::locomotion::GaitParameters;
use crate::material::{ALL_MATERIALS, CharacterMaterial, CompiledPalette};
use crate::skeleton::{BONE_COUNT, BoneId, Skeleton};

/// One compiled body part: a rigid voxel volume and the mesh of its surface.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledPart {
    volume: PartVolume,
    mesh: Mesh,
    solid_voxels: u32,
    material_counts: [u32; ALL_MATERIALS.len()],
}

impl CompiledPart {
    #[must_use]
    pub const fn bone(&self) -> BoneId {
        self.volume.bone
    }

    #[must_use]
    pub const fn volume(&self) -> &PartVolume {
        &self.volume
    }

    #[must_use]
    pub const fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub const fn solid_voxels(&self) -> u32 {
        self.solid_voxels
    }

    /// Voxel counts per material, in [`ALL_MATERIALS`] order.
    #[must_use]
    pub const fn material_counts(&self) -> &[u32; ALL_MATERIALS.len()] {
        &self.material_counts
    }

    /// The materials that actually appear on this part.
    #[must_use]
    pub fn materials(&self) -> Vec<CharacterMaterial> {
        ALL_MATERIALS
            .into_iter()
            .enumerate()
            .filter(|(index, _)| self.material_counts[*index] > 0)
            .map(|(_, material)| material)
            .collect()
    }
}

/// Everything a compiled character is.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledCharacter {
    identity: CharacterIdentity,
    body: BodyMetrics,
    skeleton: Skeleton,
    parts: Vec<CompiledPart>,
    palette: CompiledPalette,
    collision: CollisionRepresentation,
    gait: GaitParameters,
    geometry_fingerprint: u64,
    skeleton_fingerprint: u64,
    collision_fingerprint: u64,
}

impl CompiledCharacter {
    #[must_use]
    pub const fn identity(&self) -> CharacterIdentity {
        self.identity
    }

    #[must_use]
    pub const fn body(&self) -> &BodyMetrics {
        &self.body
    }

    #[must_use]
    pub const fn skeleton(&self) -> &Skeleton {
        &self.skeleton
    }

    /// The sixteen parts, in bone order.
    #[must_use]
    pub fn parts(&self) -> &[CompiledPart] {
        &self.parts
    }

    #[must_use]
    pub fn part(&self, bone: BoneId) -> &CompiledPart {
        &self.parts[bone.index()]
    }

    #[must_use]
    pub const fn palette(&self) -> &CompiledPalette {
        &self.palette
    }

    #[must_use]
    pub const fn collision(&self) -> &CollisionRepresentation {
        &self.collision
    }

    #[must_use]
    pub const fn gait(&self) -> &GaitParameters {
        &self.gait
    }

    /// One cheap value covering the descriptor and both contract versions.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        self.identity.fingerprint()
    }

    /// Hash of every compiled voxel surface, locked by the fixtures.
    #[must_use]
    pub const fn geometry_fingerprint(&self) -> u64 {
        self.geometry_fingerprint
    }

    /// Hash of the bone hierarchy and its rest pose.
    #[must_use]
    pub const fn skeleton_fingerprint(&self) -> u64 {
        self.skeleton_fingerprint
    }

    /// Hash of the capsule and every part box.
    #[must_use]
    pub const fn collision_fingerprint(&self) -> u64 {
        self.collision_fingerprint
    }

    /// Total quads across every part.
    #[must_use]
    pub fn quad_count(&self) -> usize {
        self.parts.iter().map(|part| part.mesh.quad_count()).sum()
    }

    /// Logical CPU bytes of the compiled meshes.
    #[must_use]
    pub fn mesh_payload_bytes(&self) -> usize {
        self.parts
            .iter()
            .map(|part| part.mesh.payload_bytes())
            .sum()
    }

    /// Logical bytes of the skeleton.
    #[must_use]
    pub const fn skeleton_payload_bytes(&self) -> usize {
        size_of::<Skeleton>()
    }

    /// Solid voxels across every part.
    #[must_use]
    pub fn solid_voxels(&self) -> u32 {
        self.parts.iter().map(CompiledPart::solid_voxels).sum()
    }
}

/// Why a character could not be compiled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CharacterError {
    Descriptor(CharacterDescriptorError),
    Geometry(GeometryError),
}

impl std::fmt::Display for CharacterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Descriptor(error) => write!(formatter, "invalid descriptor: {error}"),
            Self::Geometry(error) => write!(formatter, "invalid geometry: {error}"),
        }
    }
}

impl std::error::Error for CharacterError {}

impl From<CharacterDescriptorError> for CharacterError {
    fn from(error: CharacterDescriptorError) -> Self {
        Self::Descriptor(error)
    }
}

impl From<GeometryError> for CharacterError {
    fn from(error: GeometryError) -> Self {
        Self::Geometry(error)
    }
}

/// Compiles validated descriptors, reusing one scratch voxel grid.
///
/// The scratch is the only mutable state and it is empty between calls, so two
/// compilations in either order produce identical results.
pub struct CharacterCompiler {
    scratch: PartScratch,
}

impl Default for CharacterCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl CharacterCompiler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            scratch: PartScratch::new(),
        }
    }

    /// Transient bytes the compiler holds between parts.
    #[must_use]
    pub const fn scratch_bytes(&self) -> usize {
        self.scratch.payload_bytes()
    }

    /// Validates and compiles in one step.
    pub fn compile_descriptor(
        &mut self,
        descriptor: &CharacterDescriptor,
    ) -> Result<CompiledCharacter, CharacterError> {
        let validated = descriptor.validate()?;
        self.compile(&validated)
    }

    /// Compiles a descriptor whose invariants have already been proved.
    pub fn compile(
        &mut self,
        validated: &ValidatedDescriptor,
    ) -> Result<CompiledCharacter, CharacterError> {
        let body = *validated.body();
        let volumes = part_volumes(&body);
        validate_volumes(&volumes)?;

        let skeleton = Skeleton::derive(&body);
        let mut parts = Vec::with_capacity(BONE_COUNT);
        for volume in &volumes {
            let (mesh, solid_voxels, material_counts) = self.scratch.build(volume, &body)?;
            parts.push(CompiledPart {
                volume: *volume,
                mesh,
                solid_voxels,
                material_counts,
            });
        }

        let rest_world = skeleton.rest_world();
        let collision = CollisionRepresentation::derive(&volumes, &rest_world);
        let palette = CompiledPalette::resolve(validated.palette());
        let gait = GaitParameters::derive(&body);

        let geometry_fingerprint = geometry_fingerprint(&parts);
        let skeleton_fingerprint = skeleton_fingerprint(&skeleton);
        let collision_fingerprint = collision_fingerprint(&collision);

        Ok(CompiledCharacter {
            identity: CharacterIdentity::of(validated.descriptor()),
            body,
            skeleton,
            parts,
            palette,
            collision,
            gait,
            geometry_fingerprint,
            skeleton_fingerprint,
            collision_fingerprint,
        })
    }
}

fn geometry_fingerprint(parts: &[CompiledPart]) -> u64 {
    let mut bytes = Vec::with_capacity(1 << 16);
    for part in parts {
        push_u32(&mut bytes, part.volume.bone.index() as u32);
        for axis in 0..3 {
            push_f32(&mut bytes, part.volume.origin[axis]);
            push_i32(&mut bytes, part.volume.dims[axis]);
        }
        push_u32(&mut bytes, part.solid_voxels);
        for count in part.material_counts {
            push_u32(&mut bytes, count);
        }
        push_u32(&mut bytes, part.mesh.vertices().len() as u32);
        push_u32(&mut bytes, part.mesh.indices().len() as u32);
        for vertex in part.mesh.vertices() {
            for axis in 0..3 {
                push_f32(&mut bytes, vertex.position[axis]);
                push_f32(&mut bytes, vertex.normal[axis]);
            }
            push_u32(&mut bytes, u32::from(vertex.voxel.0));
        }
        for index in part.mesh.indices() {
            push_u32(&mut bytes, *index);
        }
    }
    fnv1a64(&bytes)
}

fn skeleton_fingerprint(skeleton: &Skeleton) -> u64 {
    let mut bytes = Vec::with_capacity(BONE_COUNT * 40);
    for bone in skeleton.bones() {
        push_u32(&mut bytes, bone.id.index() as u32);
        push_u32(
            &mut bytes,
            bone.parent.map_or(u32::MAX, |parent| parent.index() as u32),
        );
        let rest = bone.rest_local;
        for value in [
            rest.translation.x,
            rest.translation.y,
            rest.translation.z,
            rest.rotation.x,
            rest.rotation.y,
            rest.rotation.z,
            rest.rotation.w,
        ] {
            push_f32(&mut bytes, value);
        }
    }
    fnv1a64(&bytes)
}

fn collision_fingerprint(collision: &CollisionRepresentation) -> u64 {
    let mut bytes = Vec::with_capacity(BONE_COUNT * 32);
    let capsule = collision.capsule();
    for value in [capsule.radius, capsule.segment_height, capsule.base_height] {
        push_f32(&mut bytes, value);
    }
    for part in collision.boxes() {
        push_u32(&mut bytes, part.bone.index() as u32);
        for value in [
            part.centre.x,
            part.centre.y,
            part.centre.z,
            part.half_extents.x,
            part.half_extents.y,
            part.half_extents.z,
        ] {
            push_f32(&mut bytes, value);
        }
    }
    fnv1a64(&bytes)
}

/// Hash of a compiled character's whole observable output.
///
/// Used by the fixtures as one number to lock, and by the probe to report.
#[must_use]
pub fn behaviour_signature(character: &CompiledCharacter) -> u64 {
    let mut bytes = Vec::with_capacity(64);
    push_u64(&mut bytes, character.identity.fingerprint());
    push_u64(&mut bytes, character.geometry_fingerprint);
    push_u64(&mut bytes, character.skeleton_fingerprint);
    push_u64(&mut bytes, character.collision_fingerprint);
    for material in ALL_MATERIALS {
        for channel in character.palette.albedo(material) {
            push_f32(&mut bytes, channel);
        }
    }
    fnv1a64(&bytes)
}

#[cfg(test)]
mod tests {
    use super::{CharacterCompiler, CharacterError, behaviour_signature};
    use crate::descriptor::{CharacterDescriptor, CharacterSeed, Proportions};
    use crate::material::{
        ALL_MATERIALS, CharacterMaterial, GarmentScheme, HairTone, PaletteChoice, SkinTone,
        all_palette_choices, luminance,
    };
    use crate::skeleton::{ALL_BONES, BONE_COUNT, BoneId, Side};
    use std::collections::BTreeSet;

    fn compile(
        descriptor: &CharacterDescriptor,
    ) -> Result<super::CompiledCharacter, CharacterError> {
        CharacterCompiler::new().compile_descriptor(descriptor)
    }

    #[test]
    fn compiling_twice_gives_the_same_character() -> Result<(), CharacterError> {
        let descriptor = CharacterDescriptor::golden();
        let first = compile(&descriptor)?;
        let second = compile(&descriptor)?;
        assert_eq!(first, second, "compilation is not reproducible");
        assert_eq!(
            behaviour_signature(&first),
            behaviour_signature(&second),
            "the signature moved without the character moving"
        );
        Ok(())
    }

    #[test]
    fn one_compiler_reused_matches_two_fresh_ones() -> Result<(), CharacterError> {
        let golden = CharacterDescriptor::golden();
        let sturdy = CharacterDescriptor {
            proportions: Proportions::sturdy(),
            ..golden
        };

        let mut shared = CharacterCompiler::new();
        let a = shared.compile_descriptor(&golden)?;
        let b = shared.compile_descriptor(&sturdy)?;
        // Reverse order through the same scratch must not change anything.
        let mut reversed = CharacterCompiler::new();
        let b2 = reversed.compile_descriptor(&sturdy)?;
        let a2 = reversed.compile_descriptor(&golden)?;

        assert_eq!(a, a2, "order changed the golden character");
        assert_eq!(b, b2, "order changed the sturdy character");
        assert_eq!(a, compile(&golden)?);
        assert_eq!(b, compile(&sturdy)?);
        Ok(())
    }

    #[test]
    fn a_different_descriptor_compiles_to_a_different_character() -> Result<(), CharacterError> {
        let golden = compile(&CharacterDescriptor::golden())?;
        let sturdy = compile(&CharacterDescriptor {
            proportions: Proportions::sturdy(),
            ..CharacterDescriptor::golden()
        })?;
        assert_ne!(golden.geometry_fingerprint(), sturdy.geometry_fingerprint());
        assert_ne!(golden.skeleton_fingerprint(), sturdy.skeleton_fingerprint());
        assert_ne!(
            golden.collision_fingerprint(),
            sturdy.collision_fingerprint()
        );
        assert_ne!(golden.fingerprint(), sturdy.fingerprint());
        Ok(())
    }

    #[test]
    fn a_repainted_character_keeps_its_geometry_and_changes_its_identity()
    -> Result<(), CharacterError> {
        let golden = compile(&CharacterDescriptor::golden())?;
        let repainted = compile(&CharacterDescriptor {
            palette: PaletteChoice {
                skin: SkinTone::Deep,
                hair: HairTone::Flaxen,
                garment: GarmentScheme::SlateWool,
            },
            ..CharacterDescriptor::golden()
        })?;
        assert_eq!(
            golden.geometry_fingerprint(),
            repainted.geometry_fingerprint(),
            "a palette change moved a voxel"
        );
        assert_ne!(golden.fingerprint(), repainted.fingerprint());
        assert_ne!(
            behaviour_signature(&golden),
            behaviour_signature(&repainted)
        );
        Ok(())
    }

    #[test]
    fn the_compiled_character_has_one_part_per_bone() -> Result<(), CharacterError> {
        let character = compile(&CharacterDescriptor::golden())?;
        assert_eq!(character.parts().len(), BONE_COUNT);
        let bones: BTreeSet<BoneId> = character
            .parts()
            .iter()
            .map(super::CompiledPart::bone)
            .collect();
        assert_eq!(bones.len(), BONE_COUNT);
        for bone in ALL_BONES {
            assert_eq!(character.part(bone).bone(), bone);
            assert!(character.part(bone).mesh().quad_count() > 0);
        }
        Ok(())
    }

    #[test]
    fn left_and_right_parts_compile_to_identical_meshes() -> Result<(), CharacterError> {
        let character = compile(&CharacterDescriptor::golden())?;
        for bone in ALL_BONES {
            if bone.side() == Side::Centre {
                continue;
            }
            assert_eq!(
                character.part(bone).mesh(),
                character.part(bone.mirrored()).mesh(),
                "{} and its mirror differ",
                bone.name()
            );
        }
        Ok(())
    }

    #[test]
    fn no_part_shows_more_than_four_materials_and_the_body_no_more_than_ten()
    -> Result<(), CharacterError> {
        let character = compile(&CharacterDescriptor::golden())?;
        let mut whole_body = BTreeSet::new();
        for part in character.parts() {
            let materials = part.materials();
            assert!(
                materials.len() <= 4,
                "{} shows {} materials",
                part.bone().name(),
                materials.len()
            );
            assert!(!materials.is_empty());
            whole_body.extend(materials);
        }
        assert!(whole_body.len() <= ALL_MATERIALS.len());
        Ok(())
    }

    #[test]
    fn every_declared_material_is_actually_used() -> Result<(), CharacterError> {
        let character = compile(&CharacterDescriptor::golden())?;
        let mut used = BTreeSet::new();
        for part in character.parts() {
            used.extend(part.materials());
        }
        for material in ALL_MATERIALS {
            assert!(
                used.contains(&material),
                "{} is declared but never placed",
                material.name()
            );
        }
        Ok(())
    }

    /// Every material boundary a viewer can see on one surface must separate
    /// in value. The adjacency is computed from the compiled voxels rather
    /// than from a hand-written list of pairs, so a geometry change that
    /// creates a new boundary is caught by this test rather than by a capture.
    #[test]
    fn adjacent_materials_on_one_part_always_separate_in_value() -> Result<(), CharacterError> {
        let mut compiler = CharacterCompiler::new();
        for choice in all_palette_choices() {
            let character = compiler.compile_descriptor(&CharacterDescriptor {
                palette: choice,
                ..CharacterDescriptor::golden()
            })?;
            let palette = character.palette();
            let body = *character.body();
            for part in character.parts() {
                let volume = part.volume();
                let [w, h, d] = volume.dims;
                for z in 0..d {
                    for y in 0..h {
                        for x in 0..w {
                            let mine = crate::geometry::material_at(volume, &body, [x, y, z]);
                            for step in [[1, 0, 0], [0, 1, 0], [0, 0, 1]] {
                                let next = [x + step[0], y + step[1], z + step[2]];
                                if next[0] >= w || next[1] >= h || next[2] >= d {
                                    continue;
                                }
                                let theirs = crate::geometry::material_at(volume, &body, next);
                                if mine == theirs {
                                    continue;
                                }
                                let separation = (luminance(palette.albedo(mine))
                                    - luminance(palette.albedo(theirs)))
                                .abs();
                                assert!(
                                    separation >= 0.08,
                                    "{} touches {} on {} at {:?} with only {separation} of value \
                                     separation under {choice:?}",
                                    mine.name(),
                                    theirs.name(),
                                    part.bone().name(),
                                    [x, y, z]
                                );
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn a_character_carries_no_specular_anywhere() -> Result<(), CharacterError> {
        let character = compile(&CharacterDescriptor::golden())?;
        for part in character.parts() {
            for vertex in part.mesh().vertices() {
                let Some(material) = CharacterMaterial::from_voxel_id(vertex.voxel) else {
                    panic!("a character mesh carries a non-character identifier");
                };
                assert_eq!(material.specular(), 0.0);
            }
        }
        Ok(())
    }

    #[test]
    fn variation_changes_the_body_without_leaving_the_style_bands() -> Result<(), CharacterError> {
        let mut compiler = CharacterCompiler::new();
        let base = compiler.compile_descriptor(&CharacterDescriptor {
            build_variation: 0.06,
            ..CharacterDescriptor::golden()
        })?;
        let mut seen = BTreeSet::new();
        seen.insert(base.geometry_fingerprint());
        for raw in 1..24_u64 {
            let character = compiler.compile_descriptor(&CharacterDescriptor {
                build_variation: 0.06,
                seed: CharacterSeed(raw.wrapping_mul(0x9e37_79b9_7f4a_7c15)),
                ..CharacterDescriptor::golden()
            })?;
            seen.insert(character.geometry_fingerprint());
        }
        assert!(
            seen.len() > 1,
            "bounded variation produced only one body in twenty-four seeds"
        );
        Ok(())
    }

    #[test]
    fn an_invalid_descriptor_never_reaches_geometry() {
        let hostile = CharacterDescriptor {
            proportions: Proportions {
                limb_thickness_fraction: f64::NAN,
                ..Proportions::golden()
            },
            ..CharacterDescriptor::golden()
        };
        assert!(matches!(
            compile(&hostile),
            Err(CharacterError::Descriptor(_))
        ));
    }
}
