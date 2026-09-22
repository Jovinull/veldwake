//! Landmarks: generated world content that gives a direction a reason.
//!
//! One family, three silhouettes, three instances, placed by the world's own
//! rules. A landmark is not an entity, not a save, not a player edit and not a
//! quest: it is voxels the generator writes, exactly like terrain and
//! vegetation, and the same compiled description answers what it looks like,
//! what it is made of, and which columns it fills.
//!
//! The module lives inside `veldwake-procedural` rather than in a crate of its
//! own because it passes none of the three parts of the crate test in
//! `ARCHITECTURE.md`: it has the same owner as terrain (world generation), the
//! same lifecycle (a pure function of a coordinate and an identity), and no
//! build-isolation need — every consumer of this crate needs landmarks in its
//! chunks. A separate crate would also invert the dependency, because the
//! placement rules read the terrain field and the chunk generator writes the
//! result.
//!
//! What this module deliberately does not know: field of view, pixels,
//! cameras, renderers, fog shaders, player bodies, collision radii, combat and
//! sessions. It answers world questions — where is there solid landmark, what
//! does its outline look like from a point in the world — and the client turns
//! those into framing and walkability.

pub mod compile;
pub mod descriptor;
pub mod material;
pub mod plan;
pub mod visibility;

pub use compile::{CompiledMonolith, Silhouette};
pub use descriptor::{DescriptorError, MonolithDescriptor, SilhouetteClass, SpanAxis};
pub use material::{ALL_LANDMARK_MATERIALS, LANDMARK_ID_END, LANDMARK_ID_FIRST, LandmarkMaterial};
pub use plan::{
    DiscoveryOverlook, LandmarkInstance, LandmarkPlan, LandmarkRole, PlanError, Reservation,
};

use crate::hash::fnv1a64;

/// Shape of [`MonolithDescriptor`]. Bumped when a field is added, removed or
/// reinterpreted.
pub const LANDMARK_SCHEMA_VERSION: u32 = 1;
/// How a descriptor becomes voxels. Bumped when the compiler moves a voxel.
pub const LANDMARK_COMPILER_VERSION: u32 = 1;
/// How sites, the overlook and the reservations are chosen. Bumped when
/// placement moves a landmark.
pub const LANDMARK_PLAN_VERSION: u32 = 1;
/// Version of `docs/audiovisual/LANDMARK_STYLE.md`.
pub const LANDMARK_STYLE_VERSION: u32 = 1;

/// Locked signature of the golden landmark plan.
///
/// The same discipline as `TERRAIN_BEHAVIOR_SIGNATURE`: folded into the cheap
/// world fingerprint so a changed plan invalidates cached chunks, and checked
/// against a freshly derived plan by a test rather than recomputed at runtime.
///
/// **Old** none, **new** `0xe97b_ee8f_8573_7e95`, **why**: first lock, M8. The
/// value is the fingerprint of the golden plan itself — the overlook, and for
/// each landmark its descriptor, its compiled geometry, its origin and its
/// role — so a composition that moves by a single voxel invalidates the cache
/// and fails the lock test in the same change.
pub const LANDMARK_BEHAVIOR_SIGNATURE: u64 = 0xe97b_ee8f_8573_7e95;

/// The compositional controls of the landmark plan.
///
/// Art controls, like the terrain ones: semantic names with numbers behind
/// them, hashed into the world fingerprint so a changed control invalidates
/// cached chunks. They are a *composition* of one world, never universal
/// landmark rules — the golden clearing radius in particular is the measured
/// answer for one forest pocket and nothing more.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LandmarkControls {
    /// Where the discovery overlook should be, if the world allows it there.
    ///
    /// A hint, not a spawn: the plan resolves it to the nearest column the
    /// world actually permits, and the client reads the resolved overlook.
    pub overlook_hint: (i64, i64),
    /// Radius, in columns, of the overlook's vegetation reservation.
    pub overlook_clearing: i64,
    /// Columns of plant-free approach space around a landmark's footprint.
    pub apron: i64,
    /// Spacing of the site candidate lattice, in columns.
    pub site_lattice: i64,
    /// How far the first two landmarks stand from the overlook.
    pub first_pair_distance: (i64, i64),
    /// How far the revealed landmark stands from the one that reveals it.
    pub reveal_distance: (i64, i64),
    /// Least angle, in degrees, between the first two landmarks seen from the
    /// overlook.
    pub min_separation_degrees: f64,
    /// Least distance, in columns, between any two landmarks.
    pub min_site_separation: i64,
    /// Height above the ground the world proxy observes from.
    ///
    /// A composition control, not a camera: it stands for a viewer's eye, and
    /// a client test asserts it is consistent with the real follow camera.
    pub observer_eye: f64,
    /// Highest elevation, in degrees, at which a silhouette still counts as
    /// seen from the overlook. Also a composition control.
    pub max_elevation_degrees: f64,
    /// Least visible silhouette, in voxels, for a landmark that must be seen.
    pub min_visible_voxels: i64,
    /// Most visible silhouette, in voxels, for the landmark that must not be
    /// seen from the overlook.
    pub max_hidden_voxels: i64,
    /// Farthest a landmark may stand from where it is meant to be seen.
    ///
    /// A composition limit that keeps every sight line inside the render
    /// envelope the client actually streams; the client asserts the relation.
    pub max_sight_distance: i64,
}

impl LandmarkControls {
    /// The golden composition.
    ///
    /// The overlook hint is the clearing M5, M6 and M7 all stand in. The
    /// clearing radius is the measured answer to one specific problem — the
    /// `MeadowWood` canopy around that clearing hides everything beyond 60
    /// units at the follow camera's pitch — and is a control of this world,
    /// not a rule about landmarks.
    #[must_use]
    pub const fn golden() -> Self {
        Self {
            overlook_hint: (-69, 49),
            overlook_clearing: 40,
            apron: 12,
            site_lattice: 8,
            first_pair_distance: (60, 150),
            reveal_distance: (70, 150),
            min_separation_degrees: 60.0,
            min_site_separation: 70,
            observer_eye: 3.6,
            max_elevation_degrees: 12.0,
            min_visible_voxels: 8,
            max_hidden_voxels: 4,
            max_sight_distance: 170,
        }
    }

    /// Serialises the controls for the world fingerprint, in a fixed order.
    pub(crate) fn descriptor(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(128);
        for value in [
            self.overlook_hint.0,
            self.overlook_hint.1,
            self.overlook_clearing,
            self.apron,
            self.site_lattice,
            self.first_pair_distance.0,
            self.first_pair_distance.1,
            self.reveal_distance.0,
            self.reveal_distance.1,
            self.min_site_separation,
            self.min_visible_voxels,
            self.max_hidden_voxels,
            self.max_sight_distance,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in [
            self.min_separation_degrees,
            self.observer_eye,
            self.max_elevation_degrees,
        ] {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        bytes
    }

    /// Identity of the landmark domain under these controls.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut bytes = Vec::with_capacity(192);
        bytes.extend_from_slice(b"veldwake.landmark.controls");
        bytes.extend_from_slice(&LANDMARK_SCHEMA_VERSION.to_le_bytes());
        bytes.extend_from_slice(&LANDMARK_COMPILER_VERSION.to_le_bytes());
        bytes.extend_from_slice(&LANDMARK_PLAN_VERSION.to_le_bytes());
        bytes.extend_from_slice(&LANDMARK_STYLE_VERSION.to_le_bytes());
        bytes.extend_from_slice(&LANDMARK_BEHAVIOR_SIGNATURE.to_le_bytes());
        bytes.extend_from_slice(&self.descriptor());
        fnv1a64(&bytes)
    }
}

impl Default for LandmarkControls {
    fn default() -> Self {
        Self::golden()
    }
}
