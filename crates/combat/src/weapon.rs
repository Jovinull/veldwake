//! One weapon, compiled from a descriptor by code.
//!
//! ```text
//! WeaponDescriptor -> validate() -> ValidatedWeapon -> WeaponCompiler -> CompiledWeapon
//! ```
//!
//! A [`CompiledWeapon`] is a physical object and nothing else: dimensions, a
//! palette, a mesh, the segment its blade occupies, and the transform that puts
//! it in a hand. **It carries no timing, no damage and no reach tuning.** Those
//! belong to [`AttackSpec`](crate::spec::AttackSpec), of which M6 has two — one
//! for the player and one for the adversary — both executing this same weapon.
//! The first draft of this design put an attack profile inside the descriptor
//! while giving the two combatants different windups, which would have meant two
//! different weapons wearing one name.
//!
//! Three things here are deliberate and would be easy to get wrong:
//!
//! - **No new mesher.** The weapon is written into one reused `32³` scratch grid
//!   and meshed by `veldwake-voxel`'s existing exposed-face loop, at the
//!   character's `1/12` world unit so the two domains share one scale.
//! - **The blade's collision segment is derived from the compiled voxels**, by
//!   scanning for blade materials, rather than declared beside them. A declared
//!   length can disagree with the geometry a viewer sees; a derived one cannot.
//! - **The grip is one rigid transform.** The weapon never moves relative to the
//!   hand — the arm moves — so a weapon detaching from a hand is not a defect to
//!   watch for but a state that cannot be represented.

use glam::{Mat4, Quat, Vec3};

use veldwake_character::{CHARACTER_VOXEL_SIZE, Transform};
use veldwake_voxel::{
    BoundaryPolicy, CHUNK_EDGE, Chunk, ChunkNeighborhood, Mesh, VoxelId,
    mesh_exposed_faces_with_neighbors,
};

use crate::hash::{fnv1a64, push_f32, push_i32, push_u32, push_u64};
use crate::hit::Segment;
use crate::material::{ALL_WEAPON_MATERIALS, CompiledWeaponPalette, WeaponMaterial, WeaponScheme};

/// Version of the descriptor's shape.
pub const WEAPON_SCHEMA_VERSION: u32 = 1;
/// Version of how a descriptor becomes voxels.
pub const WEAPON_COMPILER_VERSION: u32 = 1;
/// Version of `docs/audiovisual/COMBAT_STYLE.md`.
pub const COMBAT_STYLE_VERSION: u32 = 1;

/// Largest edge, in character voxels, a weapon may occupy on any axis.
///
/// The scratch grid's edge, declared so compilation reuses exactly one grid.
pub const MAX_WEAPON_EDGE: i32 = CHUNK_EDGE as i32;

/// No weapon feature may be thinner than this on any axis.
///
/// The weapon-scale analogue of the character contract's two-voxel rule and of
/// the style bible's rejection of single-voxel terrain speckle.
pub const MIN_WEAPON_FEATURE: i32 = 2;

/// A weapon's seed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WeaponSeed(pub u64);

impl WeaponSeed {
    /// The seed every recorded weapon measurement refers to.
    pub const GOLDEN: Self = Self(0x5645_4c44_5745_4150);

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// A half-open range of grid cells on one axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Band {
    pub low: i32,
    pub high: i32,
}

impl Band {
    const fn new(low: i32, high: i32) -> Self {
        Self { low, high }
    }

    #[must_use]
    pub const fn contains(&self, value: i32) -> bool {
        value >= self.low && value < self.high
    }

    #[must_use]
    pub const fn extent(&self) -> i32 {
        self.high - self.low
    }

    /// The centre of this band, in grid units.
    #[must_use]
    pub fn centre(&self) -> f32 {
        (self.low + self.high) as f32 * 0.5
    }
}

/// Which part of a weapon a cell belongs to, for the typed rejections.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeaponPart {
    Blade,
    Guard,
    Grip,
    Pommel,
}

impl WeaponPart {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Blade => "blade",
            Self::Guard => "guard",
            Self::Grip => "grip",
            Self::Pommel => "pommel",
        }
    }
}

/// Why a descriptor cannot safely be compiled into a weapon.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeaponDescriptorError {
    SchemaMismatch {
        found: u32,
        expected: u32,
    },
    /// A feature is thinner than [`MIN_WEAPON_FEATURE`] on some axis.
    FeatureTooThin {
        part: WeaponPart,
        axis: usize,
        extent: i32,
    },
    /// A feature does not fit the scratch grid.
    FeatureTooLarge {
        part: WeaponPart,
        axis: usize,
        extent: i32,
        maximum: i32,
    },
    /// A sub-box cannot be centred on the weapon's axis without landing on a
    /// half voxel, which would put the blade off the line the grip defines.
    NotCentred {
        part: WeaponPart,
        axis: usize,
        extent: i32,
        bound: i32,
    },
    /// The guard must be at least as wide as the blade or it is not a guard.
    GuardNarrowerThanBlade {
        guard: i32,
        blade: i32,
    },
    /// The hand would sit outside the grip.
    HandOutsideGrip {
        grip_length: i32,
        grip_above_hand: i32,
    },
    /// The grip pitch is not a usable angle.
    GripPitchNotFinite,
    /// The weapon would be taller than the scratch grid.
    TooTall {
        height: i32,
        maximum: i32,
    },
}

impl std::fmt::Display for WeaponDescriptorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SchemaMismatch { found, expected } => {
                write!(formatter, "descriptor schema {found} is not {expected}")
            }
            Self::FeatureTooThin { part, axis, extent } => write!(
                formatter,
                "{} is {extent} voxels on axis {axis}, under the {MIN_WEAPON_FEATURE}-voxel minimum",
                part.name()
            ),
            Self::FeatureTooLarge {
                part,
                axis,
                extent,
                maximum,
            } => write!(
                formatter,
                "{} is {extent} voxels on axis {axis}, over the maximum {maximum}",
                part.name()
            ),
            Self::NotCentred {
                part,
                axis,
                extent,
                bound,
            } => write!(
                formatter,
                "{} is {extent} voxels on axis {axis} inside {bound}, which cannot centre",
                part.name()
            ),
            Self::GuardNarrowerThanBlade { guard, blade } => {
                write!(formatter, "guard {guard} is narrower than blade {blade}")
            }
            Self::HandOutsideGrip {
                grip_length,
                grip_above_hand,
            } => write!(
                formatter,
                "a grip of {grip_length} cannot hold a hand {grip_above_hand} voxels up"
            ),
            Self::GripPitchNotFinite => write!(formatter, "grip pitch is not finite"),
            Self::TooTall { height, maximum } => {
                write!(formatter, "weapon is {height} voxels tall, over {maximum}")
            }
        }
    }
}

impl std::error::Error for WeaponDescriptorError {}

/// What makes a weapon the object it is.
///
/// Every length is in character voxels, so a weapon and a body share one
/// lattice and one scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponDescriptor {
    pub schema_version: u32,
    pub seed: WeaponSeed,
    pub blade_length: i32,
    /// Across the blade, in the plane the swing sweeps.
    pub blade_width: i32,
    /// Across the flat, perpendicular to the swing plane.
    pub blade_thickness: i32,
    pub guard_width: i32,
    pub guard_height: i32,
    pub grip_length: i32,
    pub grip_thickness: i32,
    pub pommel_width: i32,
    pub pommel_height: i32,
    /// How many voxels of grip sit above the hand bone's own origin.
    pub grip_above_hand: i32,
    /// Pitch of the weapon in the hand, in radians about the hand's local `X`.
    ///
    /// This is what makes a carried sword point forward instead of into the
    /// ground: the blade runs down the weapon's own `-Y`, and the arm hangs, so
    /// without a grip pitch a blade long enough to reach ends up below the
    /// terrain the character is standing on.
    pub grip_pitch: f32,
    pub scheme: WeaponScheme,
}

impl WeaponDescriptor {
    /// The reference weapon every capture and every recorded number refers to.
    ///
    /// A longsword: a `14`-voxel blade is `1.167` world units, against a golden
    /// humanoid `2.333` units tall with a `1.0`-unit arm. The cross sections are
    /// even and the bounds are even so every sub-box centres exactly on the line
    /// the grip defines.
    #[must_use]
    pub const fn golden() -> Self {
        Self {
            schema_version: WEAPON_SCHEMA_VERSION,
            seed: WeaponSeed::GOLDEN,
            blade_length: 14,
            blade_width: 4,
            blade_thickness: 2,
            guard_width: 6,
            guard_height: 2,
            grip_length: 5,
            grip_thickness: 2,
            pommel_width: 4,
            pommel_height: 2,
            grip_above_hand: 2,
            // `0.90` radians. With the carry pose's own `0.90` of hand pitch the
            // blade sits a little above the hand and well forward of the body.
            grip_pitch: 0.90,
            scheme: WeaponScheme::KeenSteel,
        }
    }

    /// The bytes a fingerprint is taken over. Field order defines identity.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(64);
        push_u32(&mut bytes, self.schema_version);
        push_u64(&mut bytes, self.seed.raw());
        for value in [
            self.blade_length,
            self.blade_width,
            self.blade_thickness,
            self.guard_width,
            self.guard_height,
            self.grip_length,
            self.grip_thickness,
            self.pommel_width,
            self.pommel_height,
            self.grip_above_hand,
        ] {
            push_i32(&mut bytes, value);
        }
        push_f32(&mut bytes, self.grip_pitch);
        push_u32(&mut bytes, self.scheme as u32);
        bytes
    }

    /// Validates the descriptor and derives the voxel layout.
    pub fn validate(&self) -> Result<ValidatedWeapon, WeaponDescriptorError> {
        if self.schema_version != WEAPON_SCHEMA_VERSION {
            return Err(WeaponDescriptorError::SchemaMismatch {
                found: self.schema_version,
                expected: WEAPON_SCHEMA_VERSION,
            });
        }
        if !self.grip_pitch.is_finite() {
            return Err(WeaponDescriptorError::GripPitchNotFinite);
        }

        let parts = [
            (
                WeaponPart::Blade,
                [self.blade_thickness, self.blade_length, self.blade_width],
            ),
            (
                WeaponPart::Guard,
                [self.guard_width, self.guard_height, self.blade_width],
            ),
            (
                WeaponPart::Grip,
                [self.grip_thickness, self.grip_length, self.grip_thickness],
            ),
            (
                WeaponPart::Pommel,
                [self.pommel_width, self.pommel_height, self.pommel_width],
            ),
        ];
        for (part, extents) in parts {
            for (axis, extent) in extents.into_iter().enumerate() {
                if extent < MIN_WEAPON_FEATURE {
                    return Err(WeaponDescriptorError::FeatureTooThin { part, axis, extent });
                }
                if extent > MAX_WEAPON_EDGE {
                    return Err(WeaponDescriptorError::FeatureTooLarge {
                        part,
                        axis,
                        extent,
                        maximum: MAX_WEAPON_EDGE,
                    });
                }
            }
        }
        if self.guard_width < self.blade_width {
            return Err(WeaponDescriptorError::GuardNarrowerThanBlade {
                guard: self.guard_width,
                blade: self.blade_width,
            });
        }
        if self.grip_above_hand < 1 || self.grip_above_hand >= self.grip_length {
            return Err(WeaponDescriptorError::HandOutsideGrip {
                grip_length: self.grip_length,
                grip_above_hand: self.grip_above_hand,
            });
        }

        let height = self.pommel_height + self.grip_length + self.guard_height + self.blade_length;
        if height > MAX_WEAPON_EDGE {
            return Err(WeaponDescriptorError::TooTall {
                height,
                maximum: MAX_WEAPON_EDGE,
            });
        }

        let width = self
            .blade_thickness
            .max(self.guard_width)
            .max(self.grip_thickness)
            .max(self.pommel_width);
        let depth = self
            .blade_width
            .max(self.grip_thickness)
            .max(self.pommel_width);
        for (part, extents) in parts {
            for (axis, bound) in [(0_usize, width), (2, depth)] {
                let extent = extents[axis];
                if (bound - extent) % 2 != 0 {
                    return Err(WeaponDescriptorError::NotCentred {
                        part,
                        axis,
                        extent,
                        bound,
                    });
                }
            }
        }

        // Grid `y` counts up from the blade tip, so the tip is zero and the
        // pommel is at the top. Weapon-local `y` runs the other way, with the
        // hand's own origin at zero.
        let blade = Band::new(0, self.blade_length);
        let guard = Band::new(blade.high, blade.high + self.guard_height);
        let grip = Band::new(guard.high, guard.high + self.grip_length);
        let pommel = Band::new(grip.high, grip.high + self.pommel_height);
        let dims = [width, pommel.high, depth];
        // The hand's origin sits `grip_above_hand` voxels below the top of the
        // grip, and that is the cell the whole weapon is placed relative to.
        let hand_y = grip.high - self.grip_above_hand;
        let origin = [
            -(width as f32) * 0.5,
            -(hand_y as f32),
            -(depth as f32) * 0.5,
        ];

        let centred = |extent: i32, bound: i32| {
            let low = (bound - extent) / 2;
            Band::new(low, low + extent)
        };
        Ok(ValidatedWeapon {
            descriptor: *self,
            metrics: WeaponMetrics {
                dims,
                origin,
                hand_y,
                blade_y: blade,
                guard_y: guard,
                grip_y: grip,
                pommel_y: pommel,
                blade_x: centred(self.blade_thickness, width),
                blade_z: centred(self.blade_width, depth),
                guard_x: centred(self.guard_width, width),
                guard_z: centred(self.blade_width, depth),
                grip_x: centred(self.grip_thickness, width),
                grip_z: centred(self.grip_thickness, depth),
                pommel_x: centred(self.pommel_width, width),
                pommel_z: centred(self.pommel_width, depth),
            },
        })
    }
}

/// The derived voxel layout of one weapon, in grid cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponMetrics {
    /// Grid extent on each axis.
    pub dims: [i32; 3],
    /// Where grid cell `(0, 0, 0)` sits in the hand bone's own voxel space,
    /// before the grip transform.
    pub origin: [f32; 3],
    /// Grid `y` of the hand bone's origin.
    pub hand_y: i32,
    pub blade_y: Band,
    pub guard_y: Band,
    pub grip_y: Band,
    pub pommel_y: Band,
    pub blade_x: Band,
    pub blade_z: Band,
    pub guard_x: Band,
    pub guard_z: Band,
    pub grip_x: Band,
    pub grip_z: Band,
    pub pommel_x: Band,
    pub pommel_z: Band,
}

impl WeaponMetrics {
    /// Total cells in the volume.
    #[must_use]
    pub const fn cell_count(&self) -> i32 {
        self.dims[0] * self.dims[1] * self.dims[2]
    }

    /// The material at one grid cell, or `None` where the weapon is air.
    ///
    /// This is the one place the weapon's shape is decided, and the collision
    /// segment is derived from its output rather than from a parallel rule.
    #[must_use]
    pub fn material_at(&self, cell: [i32; 3]) -> Option<WeaponMaterial> {
        let [x, y, z] = cell;
        if self.blade_y.contains(y) && self.blade_x.contains(x) && self.blade_z.contains(z) {
            // Across the blade: the sharpened side, the flat, and the spine. The
            // edge is the low `z` face, which is the side that leads the cut.
            return Some(if z == self.blade_z.low {
                WeaponMaterial::BladeEdge
            } else if z == self.blade_z.high - 1 {
                WeaponMaterial::BladeShade
            } else {
                WeaponMaterial::BladeBody
            });
        }
        if self.guard_y.contains(y) && self.guard_x.contains(x) && self.guard_z.contains(z) {
            return Some(WeaponMaterial::Guard);
        }
        if self.grip_y.contains(y) && self.grip_x.contains(x) && self.grip_z.contains(z) {
            return Some(WeaponMaterial::Grip);
        }
        if self.pommel_y.contains(y) && self.pommel_x.contains(x) && self.pommel_z.contains(z) {
            return Some(WeaponMaterial::Pommel);
        }
        None
    }
}

/// A descriptor whose invariants have been proved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ValidatedWeapon {
    descriptor: WeaponDescriptor,
    metrics: WeaponMetrics,
}

impl ValidatedWeapon {
    #[must_use]
    pub const fn descriptor(&self) -> &WeaponDescriptor {
        &self.descriptor
    }

    #[must_use]
    pub const fn metrics(&self) -> &WeaponMetrics {
        &self.metrics
    }
}

/// The line the blade occupies, in grid units.
///
/// Derived by scanning the compiled cells for blade materials, so it is the
/// blade the viewer sees rather than a second declaration of one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BladeSegment {
    /// Where the blade meets the guard.
    pub base: Vec3,
    /// The point of the blade.
    pub tip: Vec3,
    /// Half the blade's smaller cross section, in grid units.
    pub radius: f32,
}

impl BladeSegment {
    /// Blade length in world units.
    #[must_use]
    pub fn world_length(&self) -> f32 {
        (self.tip - self.base).length() * CHARACTER_VOXEL_SIZE
    }

    /// Half-thickness in world units.
    #[must_use]
    pub fn world_radius(&self) -> f32 {
        self.radius * CHARACTER_VOXEL_SIZE
    }
}

/// A weapon's identity: the descriptor plus both contract versions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WeaponIdentity {
    pub descriptor_fingerprint: u64,
    pub compiler_version: u32,
    pub style_version: u32,
}

impl WeaponIdentity {
    #[must_use]
    pub fn of(descriptor: &WeaponDescriptor) -> Self {
        Self {
            descriptor_fingerprint: fnv1a64(&descriptor.canonical_bytes()),
            compiler_version: WEAPON_COMPILER_VERSION,
            style_version: COMBAT_STYLE_VERSION,
        }
    }

    /// One cheap value covering the descriptor and both versions.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut bytes = Vec::with_capacity(16);
        push_u64(&mut bytes, self.descriptor_fingerprint);
        push_u32(&mut bytes, self.compiler_version);
        push_u32(&mut bytes, self.style_version);
        fnv1a64(&bytes)
    }
}

/// Why a weapon could not be compiled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeaponError {
    Descriptor(WeaponDescriptorError),
    /// The layout produced no solid cell at all.
    Empty,
    /// The layout produced no blade cell, so no collision segment exists.
    NoBlade,
    /// The mesher refused the volume.
    NotMeshable,
}

impl std::fmt::Display for WeaponError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Descriptor(error) => write!(formatter, "invalid weapon descriptor: {error}"),
            Self::Empty => write!(formatter, "the weapon compiled to no voxels"),
            Self::NoBlade => write!(formatter, "the weapon compiled to no blade"),
            Self::NotMeshable => write!(formatter, "the weapon volume could not be meshed"),
        }
    }
}

impl std::error::Error for WeaponError {}

impl From<WeaponDescriptorError> for WeaponError {
    fn from(error: WeaponDescriptorError) -> Self {
        Self::Descriptor(error)
    }
}

/// Everything a compiled weapon is.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledWeapon {
    identity: WeaponIdentity,
    metrics: WeaponMetrics,
    mesh: Mesh,
    solid_voxels: u32,
    material_counts: [u32; ALL_WEAPON_MATERIALS.len()],
    palette: CompiledWeaponPalette,
    blade: BladeSegment,
    grip: Transform,
    geometry_fingerprint: u64,
}

impl CompiledWeapon {
    #[must_use]
    pub const fn identity(&self) -> WeaponIdentity {
        self.identity
    }

    #[must_use]
    pub const fn metrics(&self) -> &WeaponMetrics {
        &self.metrics
    }

    #[must_use]
    pub const fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub const fn solid_voxels(&self) -> u32 {
        self.solid_voxels
    }

    #[must_use]
    pub const fn material_counts(&self) -> &[u32; ALL_WEAPON_MATERIALS.len()] {
        &self.material_counts
    }

    #[must_use]
    pub const fn palette(&self) -> &CompiledWeaponPalette {
        &self.palette
    }

    #[must_use]
    pub const fn blade(&self) -> BladeSegment {
        self.blade
    }

    /// The rigid transform that puts this weapon in a hand.
    #[must_use]
    pub const fn grip(&self) -> Transform {
        self.grip
    }

    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        self.identity.fingerprint()
    }

    #[must_use]
    pub const fn geometry_fingerprint(&self) -> u64 {
        self.geometry_fingerprint
    }

    #[must_use]
    pub fn quad_count(&self) -> usize {
        self.mesh.quad_count()
    }

    #[must_use]
    pub fn mesh_payload_bytes(&self) -> usize {
        self.mesh.payload_bytes()
    }

    /// The world matrix of this weapon, given the wielder's placement and the
    /// world transform of the hand bone that holds it.
    ///
    /// The same composition a body part uses, with the grip inserted between the
    /// bone and the volume: nothing else is needed to draw the weapon, and the
    /// renderer composes nothing.
    #[must_use]
    pub fn matrix(&self, character_world: Mat4, hand: Transform) -> Mat4 {
        character_world
            * hand.to_mat4()
            * self.grip.to_mat4()
            * Mat4::from_translation(Vec3::from_array(self.metrics.origin))
    }

    /// The blade in world space, given the same two inputs.
    #[must_use]
    pub fn blade_world(&self, character_world: Mat4, hand: Transform) -> Segment {
        let matrix = self.matrix(character_world, hand);
        Segment::new(
            matrix.transform_point3(self.blade.base),
            matrix.transform_point3(self.blade.tip),
        )
    }

    /// Distance from the hand bone's origin to the blade's point, in world
    /// units.
    ///
    /// The arithmetic half of "can this weapon reach that body": the other half
    /// is the arm, and [`crate::spec::AttackSpec`] is checked against the sum.
    #[must_use]
    pub fn reach_from_hand(&self) -> f32 {
        let local = self
            .grip
            .transform_point(Vec3::from_array(self.metrics.origin) + self.blade.tip);
        local.length() * CHARACTER_VOXEL_SIZE
    }

    /// Blade half-thickness in world units.
    #[must_use]
    pub fn blade_radius_world(&self) -> f32 {
        self.blade.world_radius()
    }
}

/// Compiles validated weapons, reusing one scratch voxel grid.
pub struct WeaponCompiler {
    scratch: Chunk,
}

impl Default for WeaponCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl WeaponCompiler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            scratch: Chunk::empty(),
        }
    }

    /// Transient bytes this compiler holds between calls.
    #[must_use]
    pub const fn scratch_bytes(&self) -> usize {
        Chunk::BYTES
    }

    /// Validates and compiles in one step.
    pub fn compile_descriptor(
        &mut self,
        descriptor: &WeaponDescriptor,
    ) -> Result<CompiledWeapon, WeaponError> {
        let validated = descriptor.validate()?;
        self.compile(&validated)
    }

    /// Compiles a descriptor whose invariants have already been proved.
    pub fn compile(&mut self, validated: &ValidatedWeapon) -> Result<CompiledWeapon, WeaponError> {
        let metrics = validated.metrics;
        let [width, height, depth] = metrics.dims;
        let mut counts = [0_u32; ALL_WEAPON_MATERIALS.len()];
        let mut solids = 0_u32;
        // The blade's own extent, accumulated from the paint rather than from
        // the descriptor, which is what makes the segment derived.
        let mut blade_low = i32::MAX;
        let mut blade_high = i32::MIN;
        let mut blade_x = (i32::MAX, i32::MIN);
        let mut blade_z = (i32::MAX, i32::MIN);

        for z in 0..depth {
            for y in 0..height {
                for x in 0..width {
                    let Some(material) = metrics.material_at([x, y, z]) else {
                        continue;
                    };
                    counts[material.index() as usize] += 1;
                    solids += 1;
                    if material.is_blade() {
                        blade_low = blade_low.min(y);
                        blade_high = blade_high.max(y + 1);
                        blade_x = (blade_x.0.min(x), blade_x.1.max(x + 1));
                        blade_z = (blade_z.0.min(z), blade_z.1.max(z + 1));
                    }
                    if self
                        .scratch
                        .write(x as usize, y as usize, z as usize, material.voxel_id())
                        .is_err()
                    {
                        self.clear(metrics.dims);
                        return Err(WeaponError::NotMeshable);
                    }
                }
            }
        }
        if solids == 0 {
            self.clear(metrics.dims);
            return Err(WeaponError::Empty);
        }
        if blade_low > blade_high {
            self.clear(metrics.dims);
            return Err(WeaponError::NoBlade);
        }

        // `Expose` is the right policy for a free-standing object: there is no
        // neighbour, and every outward face is one a viewer can see.
        let meshed = mesh_exposed_faces_with_neighbors(
            ChunkNeighborhood::new(&self.scratch),
            BoundaryPolicy::Expose,
        );
        self.clear(metrics.dims);
        let Ok(mesh) = meshed else {
            return Err(WeaponError::NotMeshable);
        };

        let centre_x = (blade_x.0 + blade_x.1) as f32 * 0.5;
        let centre_z = (blade_z.0 + blade_z.1) as f32 * 0.5;
        let blade = BladeSegment {
            base: Vec3::new(centre_x, blade_high as f32, centre_z),
            tip: Vec3::new(centre_x, blade_low as f32, centre_z),
            radius: (blade_x.1 - blade_x.0).min(blade_z.1 - blade_z.0) as f32 * 0.5,
        };
        let grip = Transform::new(
            Quat::from_rotation_x(validated.descriptor.grip_pitch),
            Vec3::ZERO,
        );
        let palette = CompiledWeaponPalette::resolve(validated.descriptor.scheme);
        let geometry_fingerprint = geometry_fingerprint(&metrics, &mesh, solids, &counts, &blade);

        Ok(CompiledWeapon {
            identity: WeaponIdentity::of(&validated.descriptor),
            metrics,
            mesh,
            solid_voxels: solids,
            material_counts: counts,
            palette,
            blade,
            grip,
            geometry_fingerprint,
        })
    }

    /// Returns the scratch to air, exactly over the cells a compile may touch.
    fn clear(&mut self, dims: [i32; 3]) {
        let [width, height, depth] = dims;
        for z in 0..depth.min(MAX_WEAPON_EDGE) {
            for y in 0..height.min(MAX_WEAPON_EDGE) {
                for x in 0..width.min(MAX_WEAPON_EDGE) {
                    let _ = self
                        .scratch
                        .write(x as usize, y as usize, z as usize, VoxelId::AIR);
                }
            }
        }
    }
}

fn geometry_fingerprint(
    metrics: &WeaponMetrics,
    mesh: &Mesh,
    solids: u32,
    counts: &[u32; ALL_WEAPON_MATERIALS.len()],
    blade: &BladeSegment,
) -> u64 {
    let mut bytes = Vec::with_capacity(1 << 14);
    for axis in 0..3 {
        push_i32(&mut bytes, metrics.dims[axis]);
        push_f32(&mut bytes, metrics.origin[axis]);
    }
    push_i32(&mut bytes, metrics.hand_y);
    push_u32(&mut bytes, solids);
    for count in counts {
        push_u32(&mut bytes, *count);
    }
    for value in [
        blade.base.x,
        blade.base.y,
        blade.base.z,
        blade.tip.x,
        blade.tip.y,
        blade.tip.z,
        blade.radius,
    ] {
        push_f32(&mut bytes, value);
    }
    push_u32(&mut bytes, mesh.vertices().len() as u32);
    push_u32(&mut bytes, mesh.indices().len() as u32);
    for vertex in mesh.vertices() {
        for axis in 0..3 {
            push_f32(&mut bytes, vertex.position[axis]);
            push_f32(&mut bytes, vertex.normal[axis]);
        }
        push_u32(&mut bytes, u32::from(vertex.voxel.0));
    }
    for index in mesh.indices() {
        push_u32(&mut bytes, *index);
    }
    fnv1a64(&bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        Band, COMBAT_STYLE_VERSION, CompiledWeapon, MAX_WEAPON_EDGE, MIN_WEAPON_FEATURE,
        WEAPON_COMPILER_VERSION, WEAPON_SCHEMA_VERSION, WeaponCompiler, WeaponDescriptor,
        WeaponDescriptorError, WeaponError, WeaponIdentity, WeaponPart, WeaponSeed,
    };
    use crate::material::{ADJACENT_PAIRS, WeaponMaterial, WeaponScheme};
    use glam::{Mat4, Vec3};
    use std::collections::BTreeSet;
    use veldwake_character::{CHARACTER_VOXEL_SIZE, Transform};

    fn golden() -> CompiledWeapon {
        match WeaponCompiler::new().compile_descriptor(&WeaponDescriptor::golden()) {
            Ok(weapon) => weapon,
            Err(error) => panic!("the golden weapon must compile: {error}"),
        }
    }

    #[test]
    fn the_golden_weapon_compiles_to_the_layout_its_descriptor_describes() {
        let descriptor = WeaponDescriptor::golden();
        let weapon = golden();
        let metrics = weapon.metrics();
        assert_eq!(metrics.dims, [6, 23, 4]);
        assert_eq!(metrics.origin, [-3.0, -19.0, -2.0]);
        assert_eq!(metrics.hand_y, 19);
        assert_eq!(metrics.blade_y, Band::new(0, descriptor.blade_length));
        assert_eq!(metrics.guard_y, Band::new(14, 16));
        assert_eq!(metrics.grip_y, Band::new(16, 21));
        assert_eq!(metrics.pommel_y, Band::new(21, 23));
        // Solid counts are the four boxes, which is arithmetic a reader can redo
        // rather than a number to trust.
        let blade = descriptor.blade_thickness * descriptor.blade_length * descriptor.blade_width;
        let guard = descriptor.guard_width * descriptor.guard_height * descriptor.blade_width;
        let grip = descriptor.grip_thickness * descriptor.grip_length * descriptor.grip_thickness;
        let pommel = descriptor.pommel_width * descriptor.pommel_height * descriptor.pommel_width;
        assert_eq!(
            weapon.solid_voxels() as i32,
            blade + guard + grip + pommel,
            "the weapon is four boxes and nothing else"
        );
        assert!(weapon.quad_count() > 0);
        assert!(weapon.mesh_payload_bytes() > 0);
        assert_eq!(metrics.cell_count(), 6 * 23 * 4);
    }

    #[test]
    fn the_blade_segment_is_derived_from_the_painted_cells() {
        let descriptor = WeaponDescriptor::golden();
        let weapon = golden();
        let blade = weapon.blade();
        // Base at the guard, tip at the point, centred across the blade.
        assert_eq!(
            blade.base,
            Vec3::new(3.0, descriptor.blade_length as f32, 2.0)
        );
        assert_eq!(blade.tip, Vec3::new(3.0, 0.0, 2.0));
        assert_eq!(blade.radius, 1.0);
        let expected = descriptor.blade_length as f32 * CHARACTER_VOXEL_SIZE;
        assert!((blade.world_length() - expected).abs() < 1.0e-6);
        assert!((blade.world_radius() - CHARACTER_VOXEL_SIZE).abs() < 1.0e-6);
    }

    #[test]
    fn a_longer_blade_moves_the_segment_without_any_second_declaration() {
        // The point of deriving: change the geometry and the collision segment
        // follows, because there is nothing else to update.
        let mut compiler = WeaponCompiler::new();
        let long = WeaponDescriptor {
            blade_length: 20,
            ..WeaponDescriptor::golden()
        };
        let weapon = match compiler.compile_descriptor(&long) {
            Ok(weapon) => weapon,
            Err(error) => panic!("a longer blade must compile: {error}"),
        };
        assert_eq!(weapon.blade().base.y, 20.0);
        assert!(weapon.blade().world_length() > golden().blade().world_length());
    }

    #[test]
    fn the_compiled_weapon_paints_only_the_adjacent_pairs_the_palette_checks() {
        // The palette's value-separation rule is checked against a declared list
        // of pairs. This is what stops that list going stale: every pair the
        // geometry actually produces must be in it.
        let weapon = golden();
        let metrics = weapon.metrics();
        let [width, height, depth] = metrics.dims;
        let mut found: BTreeSet<(WeaponMaterial, WeaponMaterial)> = BTreeSet::new();
        for z in 0..depth {
            for y in 0..height {
                for x in 0..width {
                    let Some(here) = metrics.material_at([x, y, z]) else {
                        continue;
                    };
                    for offset in [
                        [1, 0, 0],
                        [-1, 0, 0],
                        [0, 1, 0],
                        [0, -1, 0],
                        [0, 0, 1],
                        [0, 0, -1],
                    ] {
                        let neighbour = [x + offset[0], y + offset[1], z + offset[2]];
                        let Some(there) = metrics.material_at(neighbour) else {
                            continue;
                        };
                        if here == there {
                            continue;
                        }
                        let pair = if here < there {
                            (here, there)
                        } else {
                            (there, here)
                        };
                        found.insert(pair);
                    }
                }
            }
        }
        let declared: BTreeSet<(WeaponMaterial, WeaponMaterial)> = ADJACENT_PAIRS
            .into_iter()
            .map(|(a, b)| if a < b { (a, b) } else { (b, a) })
            .collect();
        for pair in &found {
            assert!(
                declared.contains(pair),
                "{} meets {} on the weapon but the palette never checks that pair",
                pair.0.name(),
                pair.1.name()
            );
        }
        assert!(
            !found.is_empty(),
            "a weapon with one material proves nothing"
        );
    }

    #[test]
    fn the_weapon_is_one_connected_piece() {
        // Four boxes stacked: if a band stopped overlapping its neighbour the
        // weapon would render as floating parts.
        let weapon = golden();
        let metrics = weapon.metrics();
        let touching = |lower: Band, upper: Band, low_x: Band, up_x: Band| {
            assert_eq!(lower.high, upper.low, "bands must meet without a gap");
            assert!(
                low_x.low < up_x.high && up_x.low < low_x.high,
                "bands must overlap across the weapon"
            );
        };
        touching(
            metrics.blade_y,
            metrics.guard_y,
            metrics.blade_x,
            metrics.guard_x,
        );
        touching(
            metrics.guard_y,
            metrics.grip_y,
            metrics.guard_x,
            metrics.grip_x,
        );
        touching(
            metrics.grip_y,
            metrics.pommel_y,
            metrics.grip_x,
            metrics.pommel_x,
        );
    }

    #[test]
    fn the_hand_sits_inside_the_grip() {
        let weapon = golden();
        let metrics = weapon.metrics();
        assert!(
            metrics.grip_y.contains(metrics.hand_y),
            "the hand's own origin must be a grip cell"
        );
    }

    #[test]
    fn no_feature_is_thinner_than_the_style_minimum() {
        let descriptor = WeaponDescriptor::golden();
        for extent in [
            descriptor.blade_length,
            descriptor.blade_width,
            descriptor.blade_thickness,
            descriptor.guard_width,
            descriptor.guard_height,
            descriptor.grip_length,
            descriptor.grip_thickness,
            descriptor.pommel_width,
            descriptor.pommel_height,
        ] {
            assert!(
                extent >= MIN_WEAPON_FEATURE,
                "{extent} is a one-voxel feature"
            );
        }
    }

    #[test]
    fn compiling_twice_gives_the_same_weapon_because_the_scratch_is_left_clean() {
        let mut compiler = WeaponCompiler::new();
        let first = match compiler.compile_descriptor(&WeaponDescriptor::golden()) {
            Ok(weapon) => weapon,
            Err(error) => panic!("{error}"),
        };
        let other = WeaponDescriptor {
            blade_length: 18,
            scheme: WeaponScheme::DarkIron,
            ..WeaponDescriptor::golden()
        };
        let _ = compiler.compile_descriptor(&other);
        let again = match compiler.compile_descriptor(&WeaponDescriptor::golden()) {
            Ok(weapon) => weapon,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(first, again, "a dirty scratch would show up here");
        assert_eq!(compiler.scratch_bytes(), 65_536);
    }

    #[test]
    fn every_descriptor_rejection_is_reachable_and_typed() {
        let golden = WeaponDescriptor::golden();
        let cases: [(WeaponDescriptor, WeaponDescriptorError); 8] = [
            (
                WeaponDescriptor {
                    schema_version: 99,
                    ..golden
                },
                WeaponDescriptorError::SchemaMismatch {
                    found: 99,
                    expected: WEAPON_SCHEMA_VERSION,
                },
            ),
            (
                WeaponDescriptor {
                    blade_thickness: 1,
                    ..golden
                },
                WeaponDescriptorError::FeatureTooThin {
                    part: WeaponPart::Blade,
                    axis: 0,
                    extent: 1,
                },
            ),
            (
                WeaponDescriptor {
                    blade_length: 64,
                    ..golden
                },
                WeaponDescriptorError::FeatureTooLarge {
                    part: WeaponPart::Blade,
                    axis: 1,
                    extent: 64,
                    maximum: MAX_WEAPON_EDGE,
                },
            ),
            (
                WeaponDescriptor {
                    guard_width: 2,
                    ..golden
                },
                WeaponDescriptorError::GuardNarrowerThanBlade { guard: 2, blade: 4 },
            ),
            (
                WeaponDescriptor {
                    grip_above_hand: 5,
                    ..golden
                },
                WeaponDescriptorError::HandOutsideGrip {
                    grip_length: 5,
                    grip_above_hand: 5,
                },
            ),
            (
                WeaponDescriptor {
                    grip_pitch: f32::NAN,
                    ..golden
                },
                WeaponDescriptorError::GripPitchNotFinite,
            ),
            (
                WeaponDescriptor {
                    blade_length: 26,
                    ..golden
                },
                WeaponDescriptorError::TooTall {
                    height: 35,
                    maximum: MAX_WEAPON_EDGE,
                },
            ),
            (
                WeaponDescriptor {
                    guard_width: 7,
                    ..golden
                },
                WeaponDescriptorError::NotCentred {
                    part: WeaponPart::Blade,
                    axis: 0,
                    extent: 2,
                    bound: 7,
                },
            ),
        ];
        for (descriptor, expected) in cases {
            match descriptor.validate() {
                Ok(_) => panic!("{expected} should have been rejected"),
                Err(error) => assert_eq!(error, expected, "wrong rejection"),
            }
            assert!(!expected.to_string().is_empty());
        }
    }

    #[test]
    fn a_rejected_descriptor_cannot_reach_the_compiler() {
        let mut compiler = WeaponCompiler::new();
        let hostile = WeaponDescriptor {
            blade_thickness: 0,
            ..WeaponDescriptor::golden()
        };
        match compiler.compile_descriptor(&hostile) {
            Ok(_) => panic!("a hostile descriptor compiled"),
            Err(error) => assert!(matches!(error, WeaponError::Descriptor(_))),
        }
        assert!(!WeaponError::Empty.to_string().is_empty());
        assert!(!WeaponError::NoBlade.to_string().is_empty());
        assert!(!WeaponError::NotMeshable.to_string().is_empty());
    }

    #[test]
    fn the_grip_puts_the_blade_forward_and_clear_of_the_ground() {
        // The arithmetic the grip pitch exists for. With the arm hanging and no
        // grip pitch the blade points straight down; the pitch turns it forward.
        let weapon = golden();
        let grip = weapon.grip();
        let tip_local =
            grip.transform_point(Vec3::from_array(weapon.metrics().origin) + weapon.blade().tip);
        assert!(
            tip_local.z < 0.0,
            "the blade must point forward, toward -Z, not {tip_local}"
        );
        assert!(
            tip_local.y.abs() < tip_local.length(),
            "the blade must not point straight down"
        );
        // Reach is the wrist-to-point distance, which the rotation preserves.
        let expected = 19.0 * CHARACTER_VOXEL_SIZE;
        assert!(
            (weapon.reach_from_hand() - expected).abs() < 1.0e-4,
            "reach {} is not {expected}",
            weapon.reach_from_hand()
        );
        assert!((weapon.blade_radius_world() - CHARACTER_VOXEL_SIZE).abs() < 1.0e-6);
    }

    #[test]
    fn the_weapon_matrix_places_the_blade_relative_to_the_hand() {
        let weapon = golden();
        let hand = Transform::from_translation(Vec3::new(0.0, 10.0, 0.0));
        let world = Mat4::from_scale(Vec3::splat(CHARACTER_VOXEL_SIZE));
        let blade = weapon.blade_world(world, hand);
        assert!(blade.is_finite());
        assert!(
            (blade.length() - weapon.blade().world_length()).abs() < 1.0e-5,
            "the matrix must not change the blade's length"
        );
        // Moving the hand moves the blade by the same amount.
        let moved = weapon.blade_world(
            world,
            Transform::from_translation(Vec3::new(0.0, 10.0, 12.0)),
        );
        let shift = moved.tip - blade.tip;
        assert!((shift.z - 1.0).abs() < 1.0e-5, "shift {shift}");
    }

    #[test]
    fn identity_folds_the_descriptor_and_both_contract_versions() {
        let golden = WeaponDescriptor::golden();
        let identity = WeaponIdentity::of(&golden);
        assert_eq!(identity.compiler_version, WEAPON_COMPILER_VERSION);
        assert_eq!(identity.style_version, COMBAT_STYLE_VERSION);
        let repainted = WeaponDescriptor {
            scheme: WeaponScheme::DarkIron,
            ..golden
        };
        assert_ne!(
            WeaponIdentity::of(&repainted).fingerprint(),
            identity.fingerprint(),
            "a repaint must move the identity"
        );
        let reseeded = WeaponDescriptor {
            seed: WeaponSeed(1),
            ..golden
        };
        assert_ne!(
            WeaponIdentity::of(&reseeded).fingerprint(),
            identity.fingerprint()
        );
        assert_eq!(
            WeaponIdentity::of(&golden).fingerprint(),
            identity.fingerprint()
        );
    }

    #[test]
    fn a_repaint_moves_the_identity_but_not_the_geometry() {
        // The same split the character crate has: a colour change must not look
        // like a shape change.
        let mut compiler = WeaponCompiler::new();
        let keen = match compiler.compile_descriptor(&WeaponDescriptor::golden()) {
            Ok(weapon) => weapon,
            Err(error) => panic!("{error}"),
        };
        let dark = match compiler.compile_descriptor(&WeaponDescriptor {
            scheme: WeaponScheme::DarkIron,
            ..WeaponDescriptor::golden()
        }) {
            Ok(weapon) => weapon,
            Err(error) => panic!("{error}"),
        };
        assert_ne!(keen.fingerprint(), dark.fingerprint());
        assert_eq!(
            keen.geometry_fingerprint(),
            dark.geometry_fingerprint(),
            "the scheme does not move a voxel"
        );
        let longer = match compiler.compile_descriptor(&WeaponDescriptor {
            blade_length: 16,
            ..WeaponDescriptor::golden()
        }) {
            Ok(weapon) => weapon,
            Err(error) => panic!("{error}"),
        };
        assert_ne!(keen.geometry_fingerprint(), longer.geometry_fingerprint());
    }

    #[test]
    fn material_counts_add_up_to_the_solid_count() {
        let weapon = golden();
        let total: u32 = weapon.material_counts().iter().sum();
        assert_eq!(total, weapon.solid_voxels());
        // Every declared material is used, or the palette describes colours
        // nothing wears.
        for (index, count) in weapon.material_counts().iter().enumerate() {
            assert!(*count > 0, "material {index} appears on no cell");
        }
    }

    #[test]
    fn a_band_answers_what_it_contains() {
        let band = Band::new(2, 5);
        assert!(!band.contains(1));
        assert!(band.contains(2));
        assert!(band.contains(4));
        assert!(!band.contains(5));
        assert_eq!(band.extent(), 3);
        assert!((band.centre() - 3.5).abs() < 1.0e-6);
    }

    #[test]
    fn every_cell_outside_the_four_bands_is_air() {
        let weapon = golden();
        let metrics = weapon.metrics();
        // Beside the grip, at grip height but outside its cross section.
        assert!(metrics.material_at([0, 18, 0]).is_none());
        // Above the pommel and below the tip.
        assert!(metrics.material_at([3, 23, 2]).is_none());
        assert!(metrics.material_at([3, -1, 2]).is_none());
        for part in [
            WeaponPart::Blade,
            WeaponPart::Guard,
            WeaponPart::Grip,
            WeaponPart::Pommel,
        ] {
            assert!(!part.name().is_empty());
        }
    }
}
