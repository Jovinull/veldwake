//! Keyboard-toggled streaming debug views.
//!
//! This module answers one question: given what the streaming runtime and the
//! bridge already know, which wireframe primitives should be drawn? It never
//! invents state. Every record state comes from
//! [`StreamingRuntime::tracked_states`](veldwake_streaming::StreamingRuntime::tracked_states),
//! every presented level from the bridge's committed stamps, and every
//! transition group from the runtime. The renderer receives primitives and
//! draws lines; it decides nothing.
//!
//! [`DebugMode::Off`] yields no primitives at all: no per-frame debug-slot
//! allocation, debug uniform write, debug draw, or share of the mesh upload
//! budget. The fixed debug pipeline/unit geometry created at startup, and any
//! reusable slots retained after a debug mode was used, still exist.

use veldwake_combat::{Encounter, SIDES};
use veldwake_streaming::{LodLevel, MeshStatus, ResidencyStatus};
use veldwake_voxel::{CHUNK_EDGE, ChunkCoord, Face};

use crate::streaming::StreamingBridge;

/// How far a `Lod1` mesh is blended toward the cool tint in [`DebugMode::Lod`].
/// High enough to read the band at a glance, low enough to keep the diagnostic
/// checkerboard legible.
const LOD_TINT_STRENGTH: f32 = 0.55;

/// Transition groups at least this large are highlighted. KI-011 records 78
/// chunks observed at the corridor start; anything at this size already means
/// a commit that spans many frames of movement.
pub const LARGE_TRANSITION_GROUP: usize = 16;

/// Which debug view is active. `Off` is the shipped rendering.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DebugMode {
    #[default]
    Off,
    /// Chunk meshes tinted by presentation level.
    Lod,
    /// One wireframe box per tracked coordinate, colored by record state.
    Residency,
    /// Presented chunk boundaries, mixed-level seams, pending replacements,
    /// and transition groups.
    Boundaries,
    /// The volumes a hit is decided by: each body's hurt capsule and each
    /// blade's own line.
    ///
    /// This is the view that answers "the blade looked like it touched and
    /// nothing happened" and its opposite, which are two of the defects a
    /// combat capture is searched for. It draws ten primitives, against the
    /// hundreds the streaming views draw, so its cost is not the concern
    /// KI-012 records.
    Combat,
}

impl DebugMode {
    /// The `F1` cycle.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::Lod,
            Self::Lod => Self::Residency,
            Self::Residency => Self::Boundaries,
            Self::Boundaries => Self::Combat,
            Self::Combat => Self::Off,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Lod => "lod",
            Self::Residency => "residency",
            Self::Boundaries => "boundaries",
            Self::Combat => "combat",
        }
    }

    /// Whether the `F2` box toggle changes anything in this mode.
    #[must_use]
    pub const fn uses_boxes(self) -> bool {
        matches!(self, Self::Residency | Self::Boundaries | Self::Combat)
    }

    /// Whether this view draws combat volumes rather than streaming state.
    #[must_use]
    pub const fn shows_combat(self) -> bool {
        matches!(self, Self::Combat)
    }

    /// How far the mesh shader blends `Lod1` toward the debug tint.
    #[must_use]
    pub const fn lod_tint(self) -> f32 {
        match self {
            Self::Lod => LOD_TINT_STRENGTH,
            _ => 0.0,
        }
    }
}

/// Which demand set a tracked coordinate belongs to. Drawn as a box inset so
/// the rings are distinguishable without a second box per chunk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Membership {
    Render,
    DependencyOnly,
    RetentionOnly,
}

impl Membership {
    /// World-unit inset on every side of the chunk box.
    #[must_use]
    pub const fn inset(self) -> f32 {
        match self {
            Self::Render => 0.0,
            Self::DependencyOnly => 3.0,
            Self::RetentionOnly => 6.0,
        }
    }
}

/// The state of one tracked record, as the runtime reports it.
///
/// Residency answers first: a record that is not CPU-resident has no
/// meaningful mesh state to show.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordState {
    LoadQueued,
    Loading,
    KnownAbsent,
    EvictPending,
    /// Resident, but its mesh waits for a neighbor that is not available yet.
    Waiting,
    Dirty,
    Meshing,
    /// A replacement is meshed and waiting for its transition group.
    CpuReady,
    /// The committed mesh is the current target; nothing is pending.
    Committed,
    /// Resident content that is not in render demand, so no mesh is wanted.
    NotRequired,
}

impl RecordState {
    #[must_use]
    pub const fn of(residency: ResidencyStatus, mesh: MeshStatus) -> Self {
        match residency {
            ResidencyStatus::LoadQueued => Self::LoadQueued,
            ResidencyStatus::Loading => Self::Loading,
            ResidencyStatus::KnownAbsent => Self::KnownAbsent,
            ResidencyStatus::EvictPending => Self::EvictPending,
            ResidencyStatus::CpuResident => match mesh {
                MeshStatus::NotRequired => Self::NotRequired,
                MeshStatus::WaitingForNeighbors => Self::Waiting,
                MeshStatus::Dirty => Self::Dirty,
                MeshStatus::Meshing => Self::Meshing,
                MeshStatus::CpuReady => Self::CpuReady,
                MeshStatus::Committed => Self::Committed,
            },
        }
    }

    /// Every state has its own color, and every color is bright enough to read
    /// against both the dark sky and the lit floor: the first residency
    /// capture with darker greys and blues was visually empty.
    #[must_use]
    pub const fn color(self) -> [f32; 3] {
        match self {
            Self::LoadQueued => [0.78, 0.78, 0.88],
            Self::Loading => [0.92, 0.92, 0.30],
            Self::KnownAbsent => [0.52, 0.52, 0.60],
            Self::EvictPending => [0.88, 0.38, 0.88],
            Self::Waiting => [1.00, 0.66, 0.16],
            Self::Dirty => [1.00, 0.34, 0.24],
            Self::Meshing => [1.00, 0.78, 0.48],
            Self::CpuReady => [0.36, 0.90, 1.00],
            Self::Committed => [0.14, 0.96, 0.56],
            Self::NotRequired => [0.46, 0.60, 0.84],
        }
    }
}

/// What a primitive means. Color follows from this and from nothing else.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugKind {
    /// `Residency`: one box per tracked coordinate.
    Record {
        membership: Membership,
        state: RecordState,
    },
    /// `Boundaries`: a presented chunk, at the level it is drawn.
    Presented(LodLevel),
    /// `Boundaries`: the face of a presented `Lod0` chunk that meets a
    /// presented neighbor drawn at `Lod1`. Emitted from the fine side only, so
    /// each mixed seam is drawn once.
    MixedSeam(Face),
    /// `Boundaries`: a render chunk whose replacement is meshed, and whether
    /// it is already uploaded.
    Replacement { staged: bool },
    /// `Boundaries`: a member of a transition group that has not committed.
    TransitionGroup { size: usize },
    /// `Combat`: the volume a blade has to touch to hurt a body.
    CombatHurt,
    /// `Combat`: the line a blade occupies right now.
    CombatBlade,
}

impl DebugKind {
    #[must_use]
    pub const fn color(self) -> [f32; 3] {
        match self {
            Self::Record { state, .. } => state.color(),
            Self::Presented(LodLevel::Lod0) => [0.30, 0.85, 0.45],
            Self::Presented(LodLevel::Lod1) => [0.35, 0.60, 1.00],
            Self::MixedSeam(_) => [1.00, 0.85, 0.20],
            Self::Replacement { staged: false } => [0.95, 0.45, 0.15],
            Self::Replacement { staged: true } => [0.96, 0.78, 0.38],
            Self::TransitionGroup { size } if size >= LARGE_TRANSITION_GROUP => [1.00, 0.25, 0.45],
            Self::TransitionGroup { .. } => [0.76, 0.36, 0.96],
            Self::CombatHurt => [0.96, 0.36, 0.36],
            Self::CombatBlade => [0.38, 0.96, 0.58],
        }
    }

    /// World-unit inset, so nested primitives on the same chunk stay readable.
    #[must_use]
    pub const fn inset(self) -> f32 {
        match self {
            Self::Record { membership, .. } => membership.inset(),
            Self::Presented(_) | Self::MixedSeam(_) | Self::CombatHurt | Self::CombatBlade => 0.0,
            Self::TransitionGroup { .. } => 2.0,
            Self::Replacement { .. } => 5.0,
        }
    }
}

/// A box on a chunk, or the outline of one of its faces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugShape {
    Box,
    Face(Face),
}

/// One wireframe primitive: where, what it means, and how to draw it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugPrimitive {
    pub coord: ChunkCoord,
    pub kind: DebugKind,
    pub shape: DebugShape,
    /// World-unit inset applied to every side of the chunk volume.
    pub inset: f32,
    pub color: [f32; 3],
    /// When set, the primitive sits at a world-space origin with this edge
    /// length instead of on a chunk.
    ///
    /// Additive rather than a second primitive type: a combat volume is drawn by
    /// exactly the pipeline, the unit geometry and the slot pool M3C3 already
    /// built, and the only thing it needs is to be somewhere that is not a chunk.
    pub world: Option<([f32; 3], f32)>,
}

impl DebugPrimitive {
    fn new(coord: ChunkCoord, kind: DebugKind, shape: DebugShape) -> Self {
        Self {
            coord,
            kind,
            shape,
            inset: kind.inset(),
            color: kind.color(),
            world: None,
        }
    }

    /// A cube at a world-space position, for a volume that is not a chunk.
    #[must_use]
    pub fn at_world(kind: DebugKind, centre: [f32; 3], edge: f32) -> Self {
        let half = edge * 0.5;
        Self {
            coord: ChunkCoord::new(0, 0, 0),
            kind,
            shape: DebugShape::Box,
            inset: 0.0,
            color: kind.color(),
            world: Some(([centre[0] - half, centre[1] - half, centre[2] - half], edge)),
        }
    }

    /// World-space origin and edge length of the volume to draw.
    #[must_use]
    pub fn placement(&self) -> ([f32; 3], f32) {
        if let Some(placement) = self.world {
            return placement;
        }
        let edge = CHUNK_EDGE as f32;
        let origin = [
            self.coord.x as f32 * edge + self.inset,
            self.coord.y as f32 * edge + self.inset,
            self.coord.z as f32 * edge + self.inset,
        ];
        (origin, edge - 2.0 * self.inset)
    }
}

/// The volumes a hit is decided by, for the `Combat` view.
///
/// Ten primitives: three cubes along each body's hurt capsule and one at each end
/// of each blade. A capsule is not a cube and a blade is not a point, so this is
/// an approximation — but it is drawn from the same values the rules use, which is
/// the whole purpose. If the blade cube is inside the body cubes and nothing
/// happened, the defect is in the rules; if it is clear of them and a hit landed,
/// the defect is in the pose.
#[must_use]
pub fn combat_primitives(encounter: &Encounter, boxes: bool) -> Vec<DebugPrimitive> {
    if !boxes {
        return Vec::new();
    }
    let mut primitives = Vec::with_capacity(10);
    for side in SIDES {
        let combatant = encounter.combatant(side);
        let capsule = combatant.hurt_capsule();
        let edge = capsule.radius * 2.0;
        let base = capsule.axis.base;
        let tip = capsule.axis.tip;
        for point in [base, (base + tip) * 0.5, tip] {
            primitives.push(DebugPrimitive::at_world(
                DebugKind::CombatHurt,
                point.to_array(),
                edge,
            ));
        }
        let blade = encounter.blade_world(side);
        let blade_edge = encounter.weapon().blade_radius_world() * 4.0;
        for point in [blade.base, blade.tip] {
            primitives.push(DebugPrimitive::at_world(
                DebugKind::CombatBlade,
                point.to_array(),
                blade_edge,
            ));
        }
    }
    primitives
}

/// Every primitive the active view wants drawn.
///
/// `Off` and `Lod` draw no lines at all: `Off` is the shipped path and `Lod`
/// only tints the meshes that are already being drawn. `boxes` is the `F2`
/// toggle and suppresses lines in the modes that use them.
#[must_use]
pub fn debug_primitives(
    bridge: &StreamingBridge,
    mode: DebugMode,
    boxes: bool,
) -> Vec<DebugPrimitive> {
    if !boxes {
        return Vec::new();
    }
    match mode {
        // `Combat` draws no streaming state: the client calls
        // `combat_primitives` for it, because the volumes it wants come from the
        // encounter and the renderer must not learn what an encounter is.
        DebugMode::Off | DebugMode::Lod | DebugMode::Combat => Vec::new(),
        DebugMode::Residency => residency_primitives(bridge),
        DebugMode::Boundaries => boundary_primitives(bridge),
    }
}

/// One box per tracked coordinate: color is the record state, inset is the
/// demand ring it belongs to.
fn residency_primitives(bridge: &StreamingBridge) -> Vec<DebugPrimitive> {
    let demand = bridge.runtime().demand();
    bridge
        .runtime()
        .tracked_states()
        .map(|tracked| {
            let membership = if demand.render.contains(&tracked.coord) {
                Membership::Render
            } else if demand.dependency.contains(&tracked.coord) {
                Membership::DependencyOnly
            } else {
                Membership::RetentionOnly
            };
            let state = RecordState::of(tracked.residency, tracked.mesh);
            DebugPrimitive::new(
                tracked.coord,
                DebugKind::Record { membership, state },
                DebugShape::Box,
            )
        })
        .collect()
}

/// Presented chunk boundaries and what is happening to them: mixed-level
/// seams, pending replacements, and the transition groups that hold commits
/// back (KI-011).
fn boundary_primitives(bridge: &StreamingBridge) -> Vec<DebugPrimitive> {
    let levels: std::collections::BTreeMap<ChunkCoord, LodLevel> = bridge
        .presented()
        .map(|(coord, stamp)| (coord, stamp.lod))
        .collect();
    let mut primitives = Vec::with_capacity(levels.len());
    for (&coord, &lod) in &levels {
        primitives.push(DebugPrimitive::new(
            coord,
            DebugKind::Presented(lod),
            DebugShape::Box,
        ));
        if lod != LodLevel::Lod0 {
            continue;
        }
        // A mixed seam is drawn once, from the fine side.
        for face in Face::ALL {
            let Some(neighbor) = coord.neighbor(face) else {
                continue;
            };
            if levels.get(&neighbor) == Some(&LodLevel::Lod1) {
                primitives.push(DebugPrimitive::new(
                    coord,
                    DebugKind::MixedSeam(face),
                    DebugShape::Face(face),
                ));
            }
        }
    }

    for tracked in bridge.runtime().tracked_states() {
        if tracked.pending {
            primitives.push(DebugPrimitive::new(
                tracked.coord,
                DebugKind::Replacement {
                    staged: bridge.is_staged(tracked.coord),
                },
                DebugShape::Box,
            ));
        }
    }

    for group in bridge.runtime().transition_groups() {
        let size = group.len();
        for coord in group {
            primitives.push(DebugPrimitive::new(
                coord,
                DebugKind::TransitionGroup { size },
                DebugShape::Box,
            ));
        }
    }
    primitives
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use glam::Vec3;
    use veldwake_streaming::{LodLevel, MeshStatus, ResidencyStatus, StreamingConfig};
    use veldwake_voxel::{ChunkCoord, Face};

    use super::{
        DebugKind, DebugMode, DebugPrimitive, DebugShape, LARGE_TRANSITION_GROUP, Membership,
        RecordState, debug_primitives,
    };
    use crate::streaming::{
        StreamingBridge, UploadBudget,
        tests::{FakePresentation, banded_bridge_at, settle},
    };

    #[test]
    fn f1_cycles_every_mode_and_returns_to_off() {
        let mut mode = DebugMode::default();
        assert_eq!(mode, DebugMode::Off);
        let mut seen = Vec::new();
        for _ in 0..5 {
            mode = mode.next();
            seen.push(mode);
        }
        assert_eq!(
            seen,
            vec![
                DebugMode::Lod,
                DebugMode::Residency,
                DebugMode::Boundaries,
                DebugMode::Combat,
                DebugMode::Off
            ]
        );
    }

    #[test]
    fn only_the_box_modes_answer_to_f2_and_only_lod_tints() {
        assert!(!DebugMode::Off.uses_boxes());
        assert!(!DebugMode::Lod.uses_boxes());
        assert!(DebugMode::Residency.uses_boxes());
        assert!(DebugMode::Boundaries.uses_boxes());
        assert_eq!(DebugMode::Off.lod_tint(), 0.0);
        assert_eq!(DebugMode::Residency.lod_tint(), 0.0);
        assert_eq!(DebugMode::Boundaries.lod_tint(), 0.0);
        assert!(DebugMode::Lod.lod_tint() > 0.0);
    }

    #[test]
    fn record_state_follows_residency_first_then_mesh_state() {
        for (residency, expected) in [
            (ResidencyStatus::LoadQueued, RecordState::LoadQueued),
            (ResidencyStatus::Loading, RecordState::Loading),
            (ResidencyStatus::KnownAbsent, RecordState::KnownAbsent),
            (ResidencyStatus::EvictPending, RecordState::EvictPending),
        ] {
            for mesh in [
                MeshStatus::NotRequired,
                MeshStatus::Dirty,
                MeshStatus::Committed,
            ] {
                assert_eq!(RecordState::of(residency, mesh), expected);
            }
        }
        for (mesh, expected) in [
            (MeshStatus::NotRequired, RecordState::NotRequired),
            (MeshStatus::WaitingForNeighbors, RecordState::Waiting),
            (MeshStatus::Dirty, RecordState::Dirty),
            (MeshStatus::Meshing, RecordState::Meshing),
            (MeshStatus::CpuReady, RecordState::CpuReady),
            (MeshStatus::Committed, RecordState::Committed),
        ] {
            assert_eq!(
                RecordState::of(ResidencyStatus::CpuResident, mesh),
                expected
            );
        }
        let colors: BTreeSet<[u32; 3]> = [
            RecordState::LoadQueued,
            RecordState::Loading,
            RecordState::KnownAbsent,
            RecordState::EvictPending,
            RecordState::Waiting,
            RecordState::Dirty,
            RecordState::Meshing,
            RecordState::CpuReady,
            RecordState::Committed,
            RecordState::NotRequired,
        ]
        .into_iter()
        .map(|state| state.color().map(f32::to_bits))
        .collect();
        assert_eq!(colors.len(), 10, "every record state needs its own color");
    }

    #[test]
    fn a_large_transition_group_is_highlighted_differently() {
        let small = DebugKind::TransitionGroup { size: 2 };
        let large = DebugKind::TransitionGroup {
            size: LARGE_TRANSITION_GROUP,
        };
        assert_ne!(small.color(), large.color());
        assert_ne!(small.color(), DebugKind::Presented(LodLevel::Lod0).color());
        assert_ne!(
            DebugKind::Presented(LodLevel::Lod0).color(),
            DebugKind::Presented(LodLevel::Lod1).color()
        );
        assert_ne!(
            DebugKind::Replacement { staged: false }.color(),
            DebugKind::Replacement { staged: true }.color()
        );
    }

    #[test]
    fn membership_rings_are_nested_by_inset() {
        assert!(Membership::Render.inset() < Membership::DependencyOnly.inset());
        assert!(Membership::DependencyOnly.inset() < Membership::RetentionOnly.inset());
    }

    #[test]
    fn placement_insets_the_volume_on_every_side() {
        let primitive = DebugPrimitive::new(
            ChunkCoord::new(-1, 0, 2),
            DebugKind::Replacement { staged: true },
            DebugShape::Box,
        );
        let (origin, edge) = primitive.placement();
        let inset = primitive.inset;
        assert_eq!(origin, [-32.0 + inset, inset, 64.0 + inset]);
        assert_eq!(edge, 32.0 - 2.0 * inset);
    }

    #[test]
    fn off_and_disabled_boxes_produce_no_primitives_at_all() {
        let mut fake = FakePresentation::default();
        let mut bridge = banded_bridge_at(Vec3::new(5.0, 5.0, 5.0));
        settle(&mut bridge, &mut fake);
        // Something is there to draw, so an empty result means the mode drew
        // nothing and not that the world is empty.
        assert!(!debug_primitives(&bridge, DebugMode::Residency, true).is_empty());

        assert!(debug_primitives(&bridge, DebugMode::Off, true).is_empty());
        assert!(debug_primitives(&bridge, DebugMode::Lod, true).is_empty());
        assert!(debug_primitives(&bridge, DebugMode::Off, false).is_empty());
        assert!(debug_primitives(&bridge, DebugMode::Residency, false).is_empty());
        assert!(debug_primitives(&bridge, DebugMode::Boundaries, false).is_empty());
    }

    #[test]
    fn a_bridge_that_has_drawn_nothing_yet_boxes_records_but_no_boundaries() {
        let bridge = match StreamingBridge::new(
            StreamingConfig::m3c_diagnostic(),
            UploadBudget::default(),
            Vec3::new(5.0, 5.0, 5.0),
        ) {
            Ok(bridge) => bridge,
            Err(error) => panic!("bridge failed to start: {error}"),
        };
        assert!(debug_primitives(&bridge, DebugMode::Boundaries, true).is_empty());
        assert!(!debug_primitives(&bridge, DebugMode::Residency, true).is_empty());
    }

    #[test]
    fn residency_draws_exactly_one_box_per_tracked_coordinate() {
        let mut fake = FakePresentation::default();
        let mut bridge = banded_bridge_at(Vec3::new(5.0, 5.0, 5.0));
        settle(&mut bridge, &mut fake);

        let primitives = debug_primitives(&bridge, DebugMode::Residency, true);
        let tracked: BTreeSet<ChunkCoord> = bridge
            .runtime()
            .tracked_states()
            .map(|state| state.coord)
            .collect();
        let boxes: BTreeSet<ChunkCoord> = primitives.iter().map(|p| p.coord).collect();
        assert_eq!(boxes, tracked, "the box set must equal the tracked set");
        assert_eq!(primitives.len(), tracked.len(), "one box per coordinate");

        let demand = bridge.runtime().demand();
        for primitive in &primitives {
            assert_eq!(primitive.shape, DebugShape::Box);
            let DebugKind::Record { membership, state } = primitive.kind else {
                panic!("residency draws only record boxes");
            };
            let expected_membership = if demand.render.contains(&primitive.coord) {
                Membership::Render
            } else if demand.dependency.contains(&primitive.coord) {
                Membership::DependencyOnly
            } else {
                Membership::RetentionOnly
            };
            assert_eq!(membership, expected_membership, "{:?}", primitive.coord);
            let residency = bridge.runtime().residency_status(primitive.coord);
            let mesh = bridge.runtime().mesh_status(primitive.coord);
            let (Some(residency), Some(mesh)) = (residency, mesh) else {
                panic!("a tracked coordinate always has both states");
            };
            assert_eq!(state, RecordState::of(residency, mesh));
            assert_eq!(primitive.color, state.color());
        }
    }

    #[test]
    fn boundaries_box_only_presented_chunks_and_detect_each_mixed_seam_once() {
        let mut fake = FakePresentation::default();
        let mut bridge = banded_bridge_at(Vec3::new(5.0, 5.0, 5.0));
        settle(&mut bridge, &mut fake);

        let presented: BTreeMap<ChunkCoord, LodLevel> = bridge
            .presented()
            .map(|(coord, stamp)| (coord, stamp.lod))
            .collect();
        assert!(
            presented.values().any(|lod| *lod == LodLevel::Lod0)
                && presented.values().any(|lod| *lod == LodLevel::Lod1),
            "the banded profile must present both levels"
        );

        let primitives = debug_primitives(&bridge, DebugMode::Boundaries, true);
        let mut boxed = BTreeSet::new();
        // `Face` has no ordering, so seams are keyed by its `Face::ALL` index.
        let mut seams: BTreeSet<(ChunkCoord, usize)> = BTreeSet::new();
        for primitive in &primitives {
            match primitive.kind {
                DebugKind::Presented(lod) => {
                    assert_eq!(presented.get(&primitive.coord), Some(&lod));
                    assert!(boxed.insert(primitive.coord), "one box per presented chunk");
                }
                DebugKind::MixedSeam(face) => {
                    assert_eq!(primitive.shape, DebugShape::Face(face));
                    assert_eq!(presented.get(&primitive.coord), Some(&LodLevel::Lod0));
                    let Some(neighbor) = primitive.coord.neighbor(face) else {
                        panic!("a drawn seam always has a neighbor");
                    };
                    assert_eq!(presented.get(&neighbor), Some(&LodLevel::Lod1));
                    assert!(
                        seams.insert((primitive.coord, face as usize)),
                        "each mixed seam is drawn once"
                    );
                }
                DebugKind::Replacement { .. } | DebugKind::TransitionGroup { .. } => {}
                DebugKind::CombatHurt | DebugKind::CombatBlade => {
                    panic!("a streaming view must never emit a combat volume")
                }
                DebugKind::Record { .. } => panic!("boundaries draws no record boxes"),
            }
        }
        assert_eq!(
            boxed,
            presented.keys().copied().collect::<BTreeSet<_>>(),
            "every presented chunk is boxed and nothing else is"
        );

        // The expected seam set, derived independently from presented levels.
        let mut expected: BTreeSet<(ChunkCoord, usize)> = BTreeSet::new();
        for (coord, lod) in &presented {
            if *lod != LodLevel::Lod0 {
                continue;
            }
            for face in Face::ALL {
                if let Some(neighbor) = coord.neighbor(face)
                    && presented.get(&neighbor) == Some(&LodLevel::Lod1)
                {
                    expected.insert((*coord, face as usize));
                }
            }
        }
        assert_eq!(seams, expected);
        assert!(!expected.is_empty(), "a settled band has mixed seams");
    }

    #[test]
    fn boundaries_highlight_the_transition_groups_that_hold_commits_back() {
        let mut fake = FakePresentation::default();
        let mut bridge = banded_bridge_at(Vec3::new(5.0, 5.0, 5.0));
        settle(&mut bridge, &mut fake);
        bridge.track_anchor(Vec3::new(37.0, 5.0, 5.0));

        let mut checked = false;
        for _ in 0..2_000 {
            if let Err(error) = bridge.update(&mut fake) {
                panic!("update failed: {error}");
            }
            let groups = bridge.runtime().transition_groups();
            if groups.is_empty() {
                continue;
            }
            let expected: BTreeSet<(ChunkCoord, usize)> = groups
                .iter()
                .flat_map(|group| group.iter().map(|coord| (*coord, group.len())))
                .collect();
            let primitives = debug_primitives(&bridge, DebugMode::Boundaries, true);
            let drawn: BTreeSet<(ChunkCoord, usize)> = primitives
                .iter()
                .filter_map(|primitive| match primitive.kind {
                    DebugKind::TransitionGroup { size } => Some((primitive.coord, size)),
                    _ => None,
                })
                .collect();
            assert_eq!(drawn, expected, "every group member is drawn with its size");
            for primitive in &primitives {
                if let DebugKind::TransitionGroup { size } = primitive.kind
                    && size >= LARGE_TRANSITION_GROUP
                {
                    assert_eq!(
                        primitive.color,
                        DebugKind::TransitionGroup {
                            size: LARGE_TRANSITION_GROUP
                        }
                        .color(),
                        "a large group keeps the highlight color"
                    );
                }
            }
            // Pending replacements are exactly the records the runtime reports.
            let pending: BTreeSet<ChunkCoord> = bridge
                .runtime()
                .tracked_states()
                .filter(|state| state.pending)
                .map(|state| state.coord)
                .collect();
            let drawn_pending: BTreeSet<ChunkCoord> = primitives
                .iter()
                .filter_map(|primitive| match primitive.kind {
                    DebugKind::Replacement { staged } => {
                        assert_eq!(staged, bridge.is_staged(primitive.coord));
                        Some(primitive.coord)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(drawn_pending, pending);
            checked = true;
            break;
        }
        assert!(checked, "the move never produced a transition group");
    }
}
