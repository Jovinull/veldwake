use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    fmt,
    time::{Duration, Instant},
};

use veldwake_voxel::{
    CHUNK_BYTES, CHUNK_EDGE, COARSE_EDGE, Chunk, ChunkCoord, Face, FaceSlab, Mesh,
    OwnedMeshingSnapshot,
};

use crate::{
    demand::{DemandError, DemandSets, StreamingConfig},
    source::{DiagnosticChunkSource, SourceChunk},
    types::{LodLevel, MeshStamp, NeighborPresentation, NeighborStamp, RequestToken, SeamContract},
    worker::{MeshJob, MeshSnapshot, Worker, WorkerJob, WorkerResult},
};

const MESH_BURST_BEFORE_LOAD: u8 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResidencyStatus {
    LoadQueued,
    Loading,
    CpuResident,
    KnownAbsent,
    EvictPending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeshStatus {
    NotRequired,
    WaitingForNeighbors,
    Dirty,
    Meshing,
    CpuReady,
    /// Nothing pending: the committed mesh is the current target.
    Committed,
}

/// One tracked record as an observer sees it, for debug visualization.
///
/// Every field is a state the runtime already owns; nothing here is derived
/// by the renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrackedState {
    pub coord: ChunkCoord,
    pub residency: ResidencyStatus,
    pub mesh: MeshStatus,
    /// Presentation level assigned to the record, drawn or not.
    pub lod: LodLevel,
    /// A committed mesh exists, so the presentation may be drawing it.
    pub committed: bool,
    /// A replacement for the current target is meshed and waiting to commit.
    pub pending: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeMetrics {
    pub load_jobs_dispatched: u64,
    pub mesh_jobs_dispatched: u64,
    pub accepted_load_results: u64,
    pub accepted_mesh_results: u64,
    pub stale_load_results: u64,
    pub stale_mesh_results: u64,
    pub fairness_load_dispatches: u64,
    pub hard_cap_blocks: u64,
    pub snapshot_bytes_dispatched: u64,
    pub cpu_evictions_finalized: u64,
    pub eviction_budget_hits: u64,
    /// Render-demand chunks whose desired level changed.
    pub lod_swaps: u64,
    /// Presentation-only invalidations that kept the committed mesh drawable
    /// while a replacement was prepared.
    pub committed_retained: u64,
    /// Committed meshes dropped immediately for a data reason.
    pub committed_dropped: u64,
    /// Transition groups committed atomically, and chunks committed by them.
    pub transition_commits: u64,
    pub transition_chunks_committed: u64,
    /// Stale mesh results attributable to a level change of the center or of
    /// a neighbor's presentation; a subset of `stale_mesh_results`, classified
    /// after the same validation. A neighbor that is no longer resident cannot
    /// be compared and is not counted.
    pub stale_lod_results: u64,
    /// Orchestration time spent building owned mesh snapshots (any level).
    pub snapshot_build: TimingStat,
    /// Portion of snapshot building spent deriving `Lod1` data: the center
    /// `downsample_2x` and the coarse/occupancy/coverage slabs.
    pub lod1_derivation: TimingStat,
    /// Worker time inside the mesher for `Lod0` jobs.
    pub worker_mesh_lod0: TimingStat,
    /// Worker time inside the mesher for `Lod1` jobs.
    pub worker_mesh_lod1: TimingStat,
}

/// Count, total, and maximum of a measured duration, in microseconds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TimingStat {
    pub count: u64,
    pub total_us: u64,
    pub max_us: u64,
}

impl TimingStat {
    pub fn record(&mut self, elapsed: Duration) {
        let micros = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        self.count += 1;
        self.total_us = self.total_us.saturating_add(micros);
        self.max_us = self.max_us.max(micros);
    }

    #[must_use]
    pub fn mean_us(&self) -> u64 {
        self.total_us.checked_div(self.count).unwrap_or(0)
    }
}

/// One aggregate observation of runtime state, cheap enough to sample per frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResidencySummary {
    pub tracked: usize,
    pub load_queued: usize,
    pub loading: usize,
    pub cpu_resident: usize,
    pub known_absent: usize,
    pub evict_pending: usize,
    pub mesh_waiting: usize,
    pub mesh_dirty: usize,
    pub mesh_meshing: usize,
    pub mesh_ready: usize,
    pub queued_loads: usize,
    pub queued_meshes: usize,
    pub jobs_in_flight: usize,
    pub reserved_load_slots: usize,
    pub resident_payload_bytes: usize,
    pub cpu_mesh_bytes: usize,
    /// Render-demand chunks desired at each level.
    pub lod0_desired: usize,
    pub lod1_desired: usize,
    /// Render-demand chunks the source confirmed absent (never presented).
    pub render_known_absent: usize,
    /// Pending (not yet committed) ready meshes at each level.
    pub lod0_ready: usize,
    pub lod1_ready: usize,
    /// Committed (drawable) meshes at each level.
    pub lod0_committed: usize,
    pub lod1_committed: usize,
    /// Committed meshes whose target differs (a replacement is in progress).
    pub transition_pending: usize,
}

#[derive(Debug)]
pub enum RuntimeError {
    Demand(DemandError),
    WorkerStart(std::io::Error),
    WorkerDisconnected,
    RequestTokenExhausted,
    GenerationExhausted {
        coord: ChunkCoord,
        kind: &'static str,
    },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Demand(error) => error.fmt(formatter),
            Self::WorkerStart(error) => {
                write!(formatter, "failed to start streaming worker: {error}")
            }
            Self::WorkerDisconnected => write!(formatter, "streaming worker disconnected"),
            Self::RequestTokenExhausted => write!(formatter, "global request token exhausted"),
            Self::GenerationExhausted { coord, kind } => write!(
                formatter,
                "{kind} generation exhausted for chunk ({}, {}, {})",
                coord.x, coord.y, coord.z
            ),
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<DemandError> for RuntimeError {
    fn from(error: DemandError) -> Self {
        Self::Demand(error)
    }
}

#[derive(Debug)]
enum ResidencyState {
    LoadQueued,
    Loading,
    CpuResident {
        chunk: Chunk,
        content_generation: u64,
    },
    KnownAbsent,
    EvictPending(Option<Chunk>),
}

#[derive(Debug)]
enum MeshState {
    NotRequired,
    WaitingForNeighbors,
    Dirty(MeshStamp),
    Meshing(MeshStamp),
    CpuReady {
        stamp: MeshStamp,
        mesh: Mesh,
    },
    /// The committed mesh already matches the target; nothing to build.
    Committed,
}

/// The mesh presentation may draw: built for a stamp that was the target when
/// it was committed, and still correct for its data even if the target level
/// or a neighbor's seam contract moved on since.
#[derive(Debug)]
struct CommittedMesh {
    stamp: MeshStamp,
    mesh: Mesh,
}

#[derive(Debug)]
struct ChunkRecord {
    token: RequestToken,
    residency: ResidencyState,
    mesh_generation: u64,
    mesh: MeshState,
    /// Desired level; kept as history while the record is retained.
    lod: LodLevel,
    /// Why the last invalidation happened, for gap attribution.
    last_invalidation: Option<InvalidationCause>,
    /// Drawable mesh; survives presentation-only invalidations.
    committed: Option<CommittedMesh>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct LoadQueueEntry {
    priority: Reverse<(u128, ChunkCoord)>,
    coord: ChunkCoord,
    token: RequestToken,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct MeshQueueEntry {
    /// `Lod0` work sorts before `Lod1` work, then nearest first.
    priority: Reverse<(u8, u128, ChunkCoord)>,
    coord: ChunkCoord,
    token: RequestToken,
    mesh_generation: u64,
    lod: LodLevel,
}

/// What `dispatch_one` should do next. Pure so the contract is testable:
/// a ready `Lod0` mesh first, then a ready load, then a ready `Lod1` mesh;
/// after `MESH_BURST_BEFORE_LOAD` consecutive meshes a ready load goes first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Dispatch {
    Mesh,
    Load,
    Idle,
}

const fn choose_dispatch(mesh: Option<LodLevel>, load_ready: bool, consecutive: u8) -> Dispatch {
    match (mesh, load_ready) {
        (Some(LodLevel::Lod0), true) if consecutive >= MESH_BURST_BEFORE_LOAD => Dispatch::Load,
        (Some(LodLevel::Lod0), _) => Dispatch::Mesh,
        (Some(LodLevel::Lod1), true) | (None, true) => Dispatch::Load,
        (Some(LodLevel::Lod1), false) => Dispatch::Mesh,
        (None, false) => Dispatch::Idle,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InFlight {
    Load,
    Mesh,
}

/// Why a record's current mesh stopped being valid. Recorded per record so
/// presentation can attribute a gap to its cause.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidationCause {
    /// The record's own desired level changed.
    LodChange,
    /// A neighbor's presentation (level or render membership) changed.
    NeighborPresentation,
    /// The record entered or left the render set.
    Membership,
    /// Content arrived or changed, or a neighbor was loaded, evicted, or
    /// replaced: the old geometry is genuinely wrong.
    Data,
}

/// Single-worker, headless owner of diagnostic CPU chunk residency.
pub struct StreamingRuntime {
    config: StreamingConfig,
    center: ChunkCoord,
    demand: DemandSets,
    records: BTreeMap<ChunkCoord, ChunkRecord>,
    next_token: u64,
    reserved_load_slots: BTreeSet<RequestToken>,
    load_queue: BinaryHeap<LoadQueueEntry>,
    mesh_queue: BinaryHeap<MeshQueueEntry>,
    consecutive_mesh_dispatches: u8,
    in_flight: Option<InFlight>,
    worker: Worker,
    metrics: RuntimeMetrics,
}

impl StreamingRuntime {
    pub fn new(config: StreamingConfig, center: ChunkCoord) -> Result<Self, RuntimeError> {
        let config = config.validate()?;
        let demand = DemandSets::around(center, config)?;
        let worker = Worker::spawn(DiagnosticChunkSource).map_err(RuntimeError::WorkerStart)?;
        let mut runtime = Self {
            config,
            center,
            demand,
            records: BTreeMap::new(),
            next_token: RequestToken::FIRST.0,
            reserved_load_slots: BTreeSet::new(),
            load_queue: BinaryHeap::new(),
            mesh_queue: BinaryHeap::new(),
            consecutive_mesh_dispatches: 0,
            in_flight: None,
            worker,
            metrics: RuntimeMetrics::default(),
        };
        runtime.create_dependency_records()?;
        Ok(runtime)
    }

    pub fn set_demand_center(&mut self, center: ChunkCoord) -> Result<(), RuntimeError> {
        if center == self.center {
            return Ok(());
        }
        let next_demand = DemandSets::around(center, self.config)?;
        self.center = center;
        self.demand = next_demand;

        let remove: Vec<_> = self
            .records
            .keys()
            .copied()
            .filter(|coord| {
                !self.demand.retention.contains(coord)
                    || (!self.demand.dependency.contains(coord)
                        && matches!(
                            self.records.get(coord).map(|record| &record.residency),
                            Some(ResidencyState::LoadQueued)
                        ))
            })
            .collect();
        for coord in remove {
            let payload = self.records.get_mut(&coord).and_then(|record| {
                let previous =
                    std::mem::replace(&mut record.residency, ResidencyState::EvictPending(None));
                record.mesh = MeshState::NotRequired;
                record.committed = None;
                match previous {
                    ResidencyState::CpuResident { chunk, .. } => Some(chunk),
                    ResidencyState::EvictPending(chunk) => chunk,
                    _ => None,
                }
            });
            if let Some(chunk) = payload {
                if let Some(record) = self.records.get_mut(&coord) {
                    record.residency = ResidencyState::EvictPending(Some(chunk));
                }
            } else {
                self.records.remove(&coord);
            }
            self.invalidate_neighbors(coord, InvalidationCause::Data)?;
        }

        self.create_dependency_records()?;

        let render: Vec<_> = self.demand.render.iter().copied().collect();
        for coord in render {
            let Some(record) = self.records.get(&coord) else {
                continue;
            };
            let next = self.select_lod(coord, Some(record.lod));
            if next != record.lod {
                if let Some(record) = self.records.get_mut(&coord) {
                    record.lod = next;
                }
                self.metrics.lod_swaps += 1;
                self.invalidate_mesh(coord, InvalidationCause::LodChange)?;
                self.invalidate_neighbors(coord, InvalidationCause::NeighborPresentation)?;
            }
        }

        let tracked: Vec<_> = self.records.keys().copied().collect();
        for coord in tracked {
            let should_render = self.demand.render.contains(&coord);
            let currently_required = !matches!(
                self.records.get(&coord).map(|record| &record.mesh),
                Some(MeshState::NotRequired)
            );
            if should_render != currently_required {
                self.invalidate_mesh(coord, InvalidationCause::Membership)?;
            }
        }
        self.rebuild_queued_priorities();
        Ok(())
    }

    /// Integrates available results and dispatches at most one new job without blocking.
    pub fn poll(&mut self) -> Result<(), RuntimeError> {
        for _ in 0..4 {
            let Some(result) = self
                .worker
                .try_result()
                .map_err(|()| RuntimeError::WorkerDisconnected)?
            else {
                break;
            };
            self.in_flight = None;
            self.integrate_result(result)?;
        }
        self.finalize_evictions();
        if self.in_flight.is_none() {
            self.dispatch_one()?;
        }
        Ok(())
    }

    #[must_use]
    pub const fn metrics(&self) -> &RuntimeMetrics {
        &self.metrics
    }

    #[must_use]
    pub const fn demand(&self) -> &DemandSets {
        &self.demand
    }

    #[must_use]
    pub const fn center(&self) -> ChunkCoord {
        self.center
    }

    #[must_use]
    pub fn residency_status(&self, coord: ChunkCoord) -> Option<ResidencyStatus> {
        self.records.get(&coord).map(residency_status_of)
    }

    #[must_use]
    pub fn mesh_status(&self, coord: ChunkCoord) -> Option<MeshStatus> {
        self.records.get(&coord).map(mesh_status_of)
    }

    /// Every tracked record with the states the runtime already owns.
    ///
    /// This exists so a debug visualization can show residency without
    /// inventing or duplicating state: the caller reads what the runtime
    /// decided, never a renderer-side guess.
    pub fn tracked_states(&self) -> impl Iterator<Item = TrackedState> + '_ {
        self.records.iter().map(|(coord, record)| TrackedState {
            coord: *coord,
            residency: residency_status_of(record),
            mesh: mesh_status_of(record),
            lod: record.lod,
            committed: record.committed.is_some(),
            pending: matches!(record.mesh, MeshState::CpuReady { .. }),
        })
    }

    /// The pending replacement for `coord`, built for the current target.
    #[must_use]
    pub fn ready_mesh(&self, coord: ChunkCoord) -> Option<(&MeshStamp, &Mesh)> {
        let record = self.records.get(&coord)?;
        match &record.mesh {
            MeshState::CpuReady { stamp, mesh } => Some((stamp, mesh)),
            _ => None,
        }
    }

    /// The mesh presentation may draw for `coord` right now.
    #[must_use]
    pub fn committed_mesh(&self, coord: ChunkCoord) -> Option<(&MeshStamp, &Mesh)> {
        let record = self.records.get(&coord)?;
        record
            .committed
            .as_ref()
            .map(|committed| (&committed.stamp, &committed.mesh))
    }

    /// Pending replacements (CPU-ready, not yet committed) for render-demand
    /// chunks. Presentation stages these without drawing them.
    pub fn render_ready_meshes(&self) -> impl Iterator<Item = (ChunkCoord, &MeshStamp, &Mesh)> {
        self.records.iter().filter_map(|(coord, record)| {
            if !self.demand.render.contains(coord) {
                return None;
            }
            match &record.mesh {
                MeshState::CpuReady { stamp, mesh } => Some((*coord, stamp, mesh)),
                _ => None,
            }
        })
    }

    /// Committed meshes of render-demand chunks: the drawable set.
    pub fn render_committed_meshes(&self) -> impl Iterator<Item = (ChunkCoord, &MeshStamp, &Mesh)> {
        self.records.iter().filter_map(|(coord, record)| {
            if !self.demand.render.contains(coord) {
                return None;
            }
            record
                .committed
                .as_ref()
                .map(|committed| (*coord, &committed.stamp, &committed.mesh))
        })
    }

    /// A render-demand record whose committed mesh is absent or no longer the
    /// target geometry; it needs a replacement before the target is shown.
    fn needs_transition(&self, coord: ChunkCoord) -> bool {
        let Some(record) = self.records.get(&coord) else {
            return false;
        };
        if !self.demand.render.contains(&coord)
            || !matches!(record.residency, ResidencyState::CpuResident { .. })
        {
            return false;
        }
        match (&record.committed, self.current_mesh_stamp(coord)) {
            (Some(committed), Some(target)) => {
                committed.stamp.geometry_key() != target.geometry_key()
            }
            (Some(_), None) => true,
            (None, _) => true,
        }
    }

    /// Per face, whether switching `coord` from its committed mesh to its
    /// target changes the seam shared with the neighbor across that face: the
    /// presented level changes, or the dependency stamp captured toward that
    /// neighbor (its seam contract, token, or content) does. An undrawn chunk
    /// changes no seam; a drawn chunk without a target changes all of them.
    fn seam_changes_by_face(&self, coord: ChunkCoord) -> [bool; 6] {
        let Some(committed) = self
            .records
            .get(&coord)
            .and_then(|record| record.committed.as_ref())
        else {
            return [false; 6];
        };
        let Some(target) = self.current_mesh_stamp(coord) else {
            return [true; 6];
        };
        if committed.stamp.lod != target.lod {
            return [true; 6];
        }
        Face::ALL.map(|face| committed.stamp.neighbor(face) != target.neighbor(face))
    }

    /// Whether the seam `coord` shares with its neighbor across `face` changes
    /// when `coord` switches from its committed mesh to its target (see
    /// [`Self::transition_groups`]).
    #[must_use]
    pub fn seam_changes(&self, coord: ChunkCoord, face: Face) -> bool {
        self.seam_changes_by_face(coord)[face as usize]
    }

    /// Atomic transition groups: connected sets of render-demand chunks that
    /// must switch to their target meshes in the same frame so that no drawn
    /// pair ever shows an incompatible seam.
    ///
    /// Two adjacent chunks that both need a transition are joined only when a
    /// drawn side's seam toward the other changes: its presented level, or the
    /// contract it applies across that face. A drawn chunk that is dirty for
    /// another face's sake does not pull its neighbor along; an entrant next
    /// to a drawn neighbor whose seam toward it stays the same appears on its
    /// own, and two undrawn entrants constrain nothing. Joining every dirty
    /// adjacency instead chained the whole moving frontier to the Lod0 ring,
    /// which continuous movement re-dirties before it can commit.
    #[must_use]
    pub fn transition_groups(&self) -> Vec<Vec<ChunkCoord>> {
        let dirty: BTreeSet<ChunkCoord> = self
            .demand
            .render
            .iter()
            .copied()
            .filter(|coord| self.needs_transition(*coord))
            .collect();
        let changes: BTreeMap<ChunkCoord, [bool; 6]> = dirty
            .iter()
            .map(|&coord| (coord, self.seam_changes_by_face(coord)))
            .collect();
        let joined = |coord: ChunkCoord, face: Face, neighbor: ChunkCoord| {
            changes
                .get(&coord)
                .is_some_and(|faces| faces[face as usize])
                || changes
                    .get(&neighbor)
                    .is_some_and(|faces| faces[face.opposite() as usize])
        };
        let mut seen = BTreeSet::new();
        let mut groups = Vec::new();
        for &start in &dirty {
            if !seen.insert(start) {
                continue;
            }
            let mut group = vec![start];
            let mut stack = vec![start];
            while let Some(coord) = stack.pop() {
                for face in Face::ALL {
                    let Some(neighbor) = coord.neighbor(face) else {
                        continue;
                    };
                    if !dirty.contains(&neighbor) || seen.contains(&neighbor) {
                        continue;
                    }
                    if joined(coord, face, neighbor) {
                        seen.insert(neighbor);
                        group.push(neighbor);
                        stack.push(neighbor);
                    }
                }
            }
            group.sort();
            groups.push(group);
        }
        groups
    }

    /// Every member of `group` has a ready replacement for its current target.
    #[must_use]
    pub fn group_is_ready(&self, group: &[ChunkCoord]) -> bool {
        // This is a frame-path preflight. Avoid allocating a temporary set;
        // transition groups are small and normally sorted, while the public
        // API still rejects non-adjacent duplicates from arbitrary callers.
        if group.is_empty()
            || group
                .iter()
                .enumerate()
                .any(|(index, coord)| group[..index].contains(coord))
        {
            return false;
        }
        group.iter().all(|coord| {
            let Some(record) = self.records.get(coord) else {
                return false;
            };
            match (&record.mesh, self.current_mesh_stamp(*coord)) {
                (MeshState::CpuReady { stamp, .. }, Some(target)) => *stamp == target,
                _ => false,
            }
        })
    }

    /// Moves every member's ready replacement into its committed slot.
    ///
    /// All-or-nothing: a group with any member that is not ready for its
    /// current target commits nothing and returns `0`. A group is the unit of
    /// seam coherence, so a partial commit could leave a drawn pair mixing an
    /// old and a new seam. The caller activates the matching staged
    /// presentation in the same frame and must have proven, before calling,
    /// that the presentation can swap every member.
    pub fn commit_group(&mut self, group: &[ChunkCoord]) -> usize {
        match self.commit_group_with(group, || Ok::<(), std::convert::Infallible>(())) {
            Ok(committed) => committed,
            Err(error) => match error {},
        }
    }

    /// Commits a ready group only after an external participant has completed
    /// its own all-or-nothing swap.
    ///
    /// Readiness (including non-empty, unique membership) is checked before
    /// `commit_external` runs. The mutable borrow of the runtime then remains
    /// held through the callback and the CPU commit, so no load result, demand
    /// update, or generation change can invalidate the preflight between the
    /// two phases. If the external swap refuses, the runtime is unchanged.
    pub fn commit_group_with<E>(
        &mut self,
        group: &[ChunkCoord],
        commit_external: impl FnOnce() -> Result<(), E>,
    ) -> Result<usize, E> {
        if !self.group_is_ready(group) {
            return Ok(0);
        }
        commit_external()?;

        let mut committed = 0;
        for coord in group {
            let Some(target) = self.current_mesh_stamp(*coord) else {
                continue;
            };
            let Some(record) = self.records.get_mut(coord) else {
                continue;
            };
            let ready =
                matches!(&record.mesh, MeshState::CpuReady { stamp, .. } if *stamp == target);
            if !ready {
                continue;
            }
            let MeshState::CpuReady { stamp, mesh } =
                std::mem::replace(&mut record.mesh, MeshState::Committed)
            else {
                continue;
            };
            record.committed = Some(CommittedMesh { stamp, mesh });
            committed += 1;
        }
        if committed > 0 {
            self.metrics.transition_commits += 1;
            self.metrics.transition_chunks_committed += committed as u64;
        }
        Ok(committed)
    }

    /// Replaces the resident content of `coord`, bumping its content
    /// generation: every mesh built from the old content, including the
    /// committed one, is invalid immediately (a data change, never a
    /// presentation change).
    pub fn replace_content(
        &mut self,
        coord: ChunkCoord,
        chunk: Chunk,
    ) -> Result<bool, RuntimeError> {
        let Some(record) = self.records.get_mut(&coord) else {
            return Ok(false);
        };
        let ResidencyState::CpuResident {
            chunk: resident,
            content_generation,
        } = &mut record.residency
        else {
            return Ok(false);
        };
        *content_generation =
            content_generation
                .checked_add(1)
                .ok_or(RuntimeError::GenerationExhausted {
                    coord,
                    kind: "content",
                })?;
        *resident = chunk;
        self.invalidate_mesh(coord, InvalidationCause::Data)?;
        self.invalidate_neighbors(coord, InvalidationCause::Data)?;
        Ok(true)
    }

    /// Retired records still waiting for their bounded release.
    #[must_use]
    pub fn eviction_backlog(&self) -> usize {
        self.records
            .values()
            .filter(|record| matches!(record.residency, ResidencyState::EvictPending(_)))
            .count()
    }

    #[must_use]
    pub fn desired_lod(&self, coord: ChunkCoord) -> Option<LodLevel> {
        self.records.get(&coord).map(|record| record.lod)
    }

    fn select_lod(&self, coord: ChunkCoord, previous: Option<LodLevel>) -> LodLevel {
        self.config
            .lod_selection
            .select(chebyshev_distance(coord, self.center), previous)
    }

    #[must_use]
    pub fn summary(&self) -> ResidencySummary {
        let mut summary = ResidencySummary {
            tracked: self.records.len(),
            queued_loads: self.load_queue.len(),
            queued_meshes: self.mesh_queue.len(),
            jobs_in_flight: usize::from(self.in_flight.is_some()),
            reserved_load_slots: self.reserved_load_slots.len(),
            ..ResidencySummary::default()
        };
        for (coord, record) in &self.records {
            if self.demand.render.contains(coord) {
                match record.lod {
                    LodLevel::Lod0 => summary.lod0_desired += 1,
                    LodLevel::Lod1 => summary.lod1_desired += 1,
                }
                if matches!(record.residency, ResidencyState::KnownAbsent) {
                    summary.render_known_absent += 1;
                }
            }
            match &record.residency {
                ResidencyState::LoadQueued => summary.load_queued += 1,
                ResidencyState::Loading => summary.loading += 1,
                ResidencyState::CpuResident { .. } => {
                    summary.cpu_resident += 1;
                    summary.resident_payload_bytes += CHUNK_BYTES;
                }
                ResidencyState::KnownAbsent => summary.known_absent += 1,
                ResidencyState::EvictPending(chunk) => {
                    summary.evict_pending += 1;
                    if chunk.is_some() {
                        summary.resident_payload_bytes += CHUNK_BYTES;
                    }
                }
            }
            match &record.mesh {
                MeshState::NotRequired => {}
                MeshState::WaitingForNeighbors => summary.mesh_waiting += 1,
                MeshState::Dirty(_) => summary.mesh_dirty += 1,
                MeshState::Meshing(_) => summary.mesh_meshing += 1,
                MeshState::CpuReady { stamp, mesh } => {
                    summary.mesh_ready += 1;
                    summary.cpu_mesh_bytes += mesh.payload_bytes();
                    match stamp.lod {
                        LodLevel::Lod0 => summary.lod0_ready += 1,
                        LodLevel::Lod1 => summary.lod1_ready += 1,
                    }
                }
                MeshState::Committed => {}
            }
            if let Some(committed) = &record.committed {
                summary.cpu_mesh_bytes += committed.mesh.payload_bytes();
                match committed.stamp.lod {
                    LodLevel::Lod0 => summary.lod0_committed += 1,
                    LodLevel::Lod1 => summary.lod1_committed += 1,
                }
                if !matches!(record.mesh, MeshState::Committed) {
                    summary.transition_pending += 1;
                }
            }
        }
        summary
    }

    #[must_use]
    pub fn request_token(&self, coord: ChunkCoord) -> Option<RequestToken> {
        self.records.get(&coord).map(|record| record.token)
    }

    #[must_use]
    pub fn resident_payload_count(&self) -> usize {
        self.records
            .values()
            .filter(|record| match &record.residency {
                ResidencyState::CpuResident { .. } => true,
                ResidencyState::EvictPending(chunk) => chunk.is_some(),
                _ => false,
            })
            .count()
    }

    #[must_use]
    pub fn reserved_load_count(&self) -> usize {
        self.reserved_load_slots.len()
    }

    #[must_use]
    pub fn logical_resident_bytes(&self) -> usize {
        self.resident_payload_count() * CHUNK_BYTES
    }

    #[must_use]
    pub fn tracked_count(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn ready_mesh_count(&self) -> usize {
        self.records
            .values()
            .filter(|record| matches!(record.mesh, MeshState::CpuReady { .. }))
            .count()
    }

    #[must_use]
    pub fn committed_mesh_count(&self) -> usize {
        self.records
            .values()
            .filter(|record| record.committed.is_some())
            .count()
    }

    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.in_flight.is_none()
            && !self.records.values().any(|record| {
                matches!(
                    record.residency,
                    ResidencyState::LoadQueued | ResidencyState::Loading
                ) || matches!(
                    record.mesh,
                    MeshState::Dirty(_) | MeshState::Meshing(_) | MeshState::WaitingForNeighbors
                )
            })
    }

    fn create_dependency_records(&mut self) -> Result<(), RuntimeError> {
        let reincarnated: Vec<_> = self
            .demand
            .dependency
            .iter()
            .copied()
            .filter(|coord| {
                matches!(
                    self.records.get(coord).map(|record| &record.residency),
                    Some(ResidencyState::EvictPending(_))
                )
            })
            .collect();
        for coord in reincarnated {
            self.records.remove(&coord);
        }
        let missing: Vec<_> = self
            .demand
            .dependency
            .iter()
            .copied()
            .filter(|coord| !self.records.contains_key(coord))
            .collect();
        for coord in missing {
            let token = self.allocate_token()?;
            let lod = self.select_lod(coord, None);
            self.records.insert(
                coord,
                ChunkRecord {
                    token,
                    residency: ResidencyState::LoadQueued,
                    mesh_generation: 0,
                    mesh: MeshState::NotRequired,
                    lod,
                    last_invalidation: None,
                    committed: None,
                },
            );
            self.load_queue.push(LoadQueueEntry {
                priority: self.priority(coord),
                coord,
                token,
            });
            if self.demand.render.contains(&coord) {
                self.invalidate_mesh(coord, InvalidationCause::Membership)?;
            }
        }
        Ok(())
    }

    /// Releases at most `max_cpu_evictions_per_update` retired records.
    ///
    /// A retired record still owning a `Chunk` keeps consuming hard-cap budget,
    /// so a large backlog throttles new loads instead of breaching the cap.
    fn finalize_evictions(&mut self) {
        let budget = self.config.max_cpu_evictions_per_update;
        let evicted: Vec<_> = self
            .records
            .iter()
            .filter_map(|(coord, record)| {
                matches!(record.residency, ResidencyState::EvictPending(_)).then_some(*coord)
            })
            .take(budget)
            .collect();
        for coord in &evicted {
            self.records.remove(coord);
        }
        self.metrics.cpu_evictions_finalized += evicted.len() as u64;
        if evicted.len() == budget && self.eviction_backlog() > 0 {
            self.metrics.eviction_budget_hits += 1;
        }
    }

    fn allocate_token(&mut self) -> Result<RequestToken, RuntimeError> {
        let token = RequestToken(self.next_token);
        self.next_token = self
            .next_token
            .checked_add(1)
            .ok_or(RuntimeError::RequestTokenExhausted)?;
        Ok(token)
    }

    fn rebuild_queued_priorities(&mut self) {
        self.load_queue.clear();
        self.mesh_queue.clear();
        for (&coord, record) in &self.records {
            if matches!(record.residency, ResidencyState::LoadQueued)
                && self.demand.dependency.contains(&coord)
            {
                self.load_queue.push(LoadQueueEntry {
                    priority: self.priority(coord),
                    coord,
                    token: record.token,
                });
            }
            if let MeshState::Dirty(stamp) = record.mesh {
                self.mesh_queue.push(MeshQueueEntry {
                    priority: self.mesh_priority(coord, stamp.lod),
                    coord,
                    token: record.token,
                    mesh_generation: stamp.mesh_generation,
                    lod: stamp.lod,
                });
            }
        }
    }

    const fn cause_is_presentation_only(cause: InvalidationCause) -> bool {
        matches!(
            cause,
            InvalidationCause::LodChange
                | InvalidationCause::NeighborPresentation
                | InvalidationCause::Membership
        )
    }

    fn invalidate_neighbors(
        &mut self,
        coord: ChunkCoord,
        cause: InvalidationCause,
    ) -> Result<(), RuntimeError> {
        for face in Face::ALL {
            if let Some(neighbor) = coord.neighbor(face) {
                self.invalidate_mesh(neighbor, cause)?;
            }
        }
        Ok(())
    }

    /// Cause of the last invalidation of `coord`, if any.
    #[must_use]
    pub fn last_invalidation(&self, coord: ChunkCoord) -> Option<InvalidationCause> {
        self.records
            .get(&coord)
            .and_then(|record| record.last_invalidation)
    }

    fn invalidate_mesh(
        &mut self,
        coord: ChunkCoord,
        cause: InvalidationCause,
    ) -> Result<(), RuntimeError> {
        let Some(record) = self.records.get_mut(&coord) else {
            return Ok(());
        };
        record.last_invalidation = Some(cause);
        record.mesh_generation =
            record
                .mesh_generation
                .checked_add(1)
                .ok_or(RuntimeError::GenerationExhausted {
                    coord,
                    kind: "mesh",
                })?;
        if !self.demand.render.contains(&coord)
            || !matches!(record.residency, ResidencyState::CpuResident { .. })
        {
            // Leaving render demand, or losing resident content, ends the
            // committed mesh: nothing outside the visible set is drawn, and a
            // mesh of data that is gone is wrong.
            if record.committed.take().is_some() {
                self.metrics.committed_dropped += 1;
            }
            // A source-confirmed absence has nothing to mesh, ever; only data
            // that is still arriving is genuinely waiting.
            let waiting_for_data = self.demand.render.contains(&coord)
                && matches!(
                    record.residency,
                    ResidencyState::LoadQueued | ResidencyState::Loading
                );
            record.mesh = if waiting_for_data {
                MeshState::WaitingForNeighbors
            } else {
                MeshState::NotRequired
            };
            return Ok(());
        }

        record.mesh = MeshState::WaitingForNeighbors;
        let target = self.current_mesh_stamp(coord);
        let record = self
            .records
            .get_mut(&coord)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        // The committed mesh survives only a presentation-only change whose
        // stamp still describes the same data; anything else drops it now.
        if let Some(committed) = &record.committed {
            let safe = Self::cause_is_presentation_only(cause)
                && target
                    .as_ref()
                    .is_some_and(|target| committed.stamp.differs_only_by_presentation(target));
            if safe {
                self.metrics.committed_retained += 1;
            } else {
                record.committed = None;
                self.metrics.committed_dropped += 1;
            }
        }
        if let Some(stamp) = target {
            if record
                .committed
                .as_ref()
                .is_some_and(|committed| committed.stamp.geometry_key() == stamp.geometry_key())
            {
                // The target is what is already drawn; no replacement needed.
                record.mesh = MeshState::Committed;
                return Ok(());
            }
            record.mesh = MeshState::Dirty(stamp);
            self.mesh_queue.push(MeshQueueEntry {
                priority: self.mesh_priority(coord, stamp.lod),
                coord,
                token: stamp.request_token,
                mesh_generation: stamp.mesh_generation,
                lod: stamp.lod,
            });
        }
        Ok(())
    }

    fn current_mesh_stamp(&self, coord: ChunkCoord) -> Option<MeshStamp> {
        let record = self.records.get(&coord)?;
        let ResidencyState::CpuResident {
            content_generation, ..
        } = record.residency
        else {
            return None;
        };
        let neighbors = [
            self.neighbor_stamp(coord, Face::NegativeX)?,
            self.neighbor_stamp(coord, Face::PositiveX)?,
            self.neighbor_stamp(coord, Face::NegativeY)?,
            self.neighbor_stamp(coord, Face::PositiveY)?,
            self.neighbor_stamp(coord, Face::NegativeZ)?,
            self.neighbor_stamp(coord, Face::PositiveZ)?,
        ];
        Some(MeshStamp {
            coord,
            request_token: record.token,
            lod: record.lod,
            mesh_generation: record.mesh_generation,
            center_content_generation: content_generation,
            neighbors,
        })
    }

    fn neighbor_stamp(&self, coord: ChunkCoord, face: Face) -> Option<NeighborStamp> {
        let record = self.records.get(&coord)?;
        let neighbor_coord = coord.neighbor(face)?;
        let neighbor = self.records.get(&neighbor_coord)?;
        match neighbor.residency {
            ResidencyState::CpuResident {
                content_generation, ..
            } => Some(NeighborStamp::Resident {
                coord: neighbor_coord,
                token: neighbor.token,
                content_generation,
                seam: SeamContract::derive(
                    record.lod,
                    if self.demand.render.contains(&neighbor_coord) {
                        NeighborPresentation::Rendered(neighbor.lod)
                    } else {
                        NeighborPresentation::ContentOnly
                    },
                ),
            }),
            ResidencyState::KnownAbsent => Some(NeighborStamp::KnownAbsent {
                coord: neighbor_coord,
                token: neighbor.token,
            }),
            _ => None,
        }
    }

    /// Builds the owned job input for `stamp` at its level.
    ///
    /// Mixed-resolution seams follow the coarse-occupancy rule: a fine center
    /// samples the coarse block covering each seam cell of a `Lod1`-presented
    /// neighbor, and a coarse center always emits its seam toward a
    /// `Lod0`-presented neighbor. Content-only neighbors supply voxels at the
    /// center's own resolution; unavailable neighbors still block.
    /// Returns the snapshot and the time spent deriving `Lod1` data inside it.
    fn snapshot(&self, stamp: MeshStamp) -> Option<(MeshSnapshot, Duration)> {
        if self.current_mesh_stamp(stamp.coord)? != stamp {
            return None;
        }
        let mut derivation = Duration::ZERO;
        let mut derive = |work: &mut dyn FnMut() -> MeshSnapshotSlab| {
            let started = Instant::now();
            let slab = work();
            derivation += started.elapsed();
            slab
        };
        let center_record = self.records.get(&stamp.coord)?;
        let ResidencyState::CpuResident { chunk: center, .. } = &center_record.residency else {
            return None;
        };
        // Slabs follow the stamp's seam contracts, so the job reproduces
        // exactly the target presentation it was issued for.
        let neighbor_of = |face: Face| {
            let neighbor_coord = stamp.coord.neighbor(face)?;
            let record = self.records.get(&neighbor_coord)?;
            let seam = match stamp.neighbors[face as usize] {
                NeighborStamp::Resident { seam, .. } => seam,
                NeighborStamp::KnownAbsent { .. } => SeamContract::Same,
            };
            match &record.residency {
                ResidencyState::CpuResident { chunk, .. } => Some(Some((chunk, seam))),
                ResidencyState::KnownAbsent => Some(None),
                _ => None,
            }
        };
        match stamp.lod {
            LodLevel::Lod0 => {
                let mut slab = |face: Face| -> Option<FaceSlab<CHUNK_EDGE>> {
                    Some(match neighbor_of(face)? {
                        None => FaceSlab::known_air(),
                        Some((chunk, SeamContract::CoarseNeighbor)) => {
                            match derive(&mut || {
                                MeshSnapshotSlab::Fine(FaceSlab::coarse_occupancy_of(face, chunk))
                            }) {
                                MeshSnapshotSlab::Fine(slab) => slab,
                                MeshSnapshotSlab::Coarse(_) => unreachable!("fine derivation"),
                            }
                        }
                        Some((chunk, _)) => FaceSlab::from_neighbor(face, chunk),
                    })
                };
                let faces = [
                    slab(Face::NegativeX)?,
                    slab(Face::PositiveX)?,
                    slab(Face::NegativeY)?,
                    slab(Face::PositiveY)?,
                    slab(Face::NegativeZ)?,
                    slab(Face::PositiveZ)?,
                ];
                Some((
                    MeshSnapshot::Fine(OwnedMeshingSnapshot::new(center.clone(), faces)),
                    derivation,
                ))
            }
            LodLevel::Lod1 => {
                let mut slab = |face: Face| -> Option<FaceSlab<COARSE_EDGE>> {
                    let neighbor = neighbor_of(face)?;
                    let slab = derive(&mut || {
                        MeshSnapshotSlab::Coarse(match neighbor {
                            None => FaceSlab::known_air(),
                            // Toward a fine neighbor the coarse quad stays unless
                            // the fine seam layer covers it completely.
                            Some((chunk, SeamContract::FineNeighbor)) => {
                                FaceSlab::fine_coverage_of(face, chunk)
                            }
                            Some((chunk, _)) => FaceSlab::downsampled_from(face, chunk),
                        })
                    });
                    match slab {
                        MeshSnapshotSlab::Coarse(slab) => Some(slab),
                        MeshSnapshotSlab::Fine(_) => unreachable!("coarse derivation"),
                    }
                };
                let faces = [
                    slab(Face::NegativeX)?,
                    slab(Face::PositiveX)?,
                    slab(Face::NegativeY)?,
                    slab(Face::PositiveY)?,
                    slab(Face::NegativeZ)?,
                    slab(Face::PositiveZ)?,
                ];
                let started = Instant::now();
                let coarse_center = center.downsample_2x();
                derivation += started.elapsed();
                Some((
                    MeshSnapshot::Coarse(OwnedMeshingSnapshot::new(coarse_center, faces)),
                    derivation,
                ))
            }
        }
    }

    fn integrate_result(&mut self, result: WorkerResult) -> Result<(), RuntimeError> {
        match result {
            WorkerResult::Load {
                coord,
                token,
                source,
            } => self.integrate_load(coord, token, source),
            WorkerResult::Mesh(result) => {
                match result.stamp.lod {
                    LodLevel::Lod0 => self.metrics.worker_mesh_lod0.record(result.mesh_time),
                    LodLevel::Lod1 => self.metrics.worker_mesh_lod1.record(result.mesh_time),
                }
                self.integrate_mesh(result.stamp, result.mesh)
            }
        }
    }

    fn integrate_load(
        &mut self,
        coord: ChunkCoord,
        token: RequestToken,
        source: SourceChunk,
    ) -> Result<(), RuntimeError> {
        self.reserved_load_slots.remove(&token);
        let accepted = matches!(
            self.records.get(&coord),
            Some(ChunkRecord {
                token: current,
                residency: ResidencyState::Loading,
                ..
            }) if *current == token
        );
        if !accepted {
            self.metrics.stale_load_results += 1;
            return Ok(());
        }

        let record = self
            .records
            .get_mut(&coord)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        record.residency = match source {
            SourceChunk::Present(chunk) => ResidencyState::CpuResident {
                chunk,
                content_generation: 1,
            },
            SourceChunk::KnownAbsent => ResidencyState::KnownAbsent,
        };
        self.metrics.accepted_load_results += 1;
        self.invalidate_mesh(coord, InvalidationCause::Data)?;
        self.invalidate_neighbors(coord, InvalidationCause::Data)
    }

    fn integrate_mesh(&mut self, stamp: MeshStamp, mesh: Mesh) -> Result<(), RuntimeError> {
        let accepted = self.current_mesh_stamp(stamp.coord) == Some(stamp)
            && matches!(
                self.records.get(&stamp.coord).map(|record| &record.mesh),
                Some(MeshState::Meshing(active)) if *active == stamp
            );
        if !accepted {
            self.metrics.stale_mesh_results += 1;
            if self.is_lod_stale(stamp) {
                self.metrics.stale_lod_results += 1;
            }
            return Ok(());
        }
        let record = self
            .records
            .get_mut(&stamp.coord)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        record.mesh = MeshState::CpuReady { stamp, mesh };
        self.metrics.accepted_mesh_results += 1;
        Ok(())
    }

    /// Level change of the center, or of a neighbor's presentation, explains
    /// this stale result. Neighbors no longer resident cannot be compared.
    fn is_lod_stale(&self, stamp: MeshStamp) -> bool {
        let Some(record) = self.records.get(&stamp.coord) else {
            return false;
        };
        if record.lod != stamp.lod {
            return true;
        }
        Face::ALL.into_iter().any(|face| {
            let (
                Some(NeighborStamp::Resident { seam: current, .. }),
                NeighborStamp::Resident { seam: previous, .. },
            ) = (
                self.neighbor_stamp(stamp.coord, face),
                stamp.neighbors[face as usize],
            )
            else {
                return false;
            };
            current != previous
        })
    }

    fn dispatch_one(&mut self) -> Result<(), RuntimeError> {
        let mesh = self.pop_valid_mesh();
        let load = self.pop_valid_load();
        let choice = choose_dispatch(
            mesh.map(|entry| entry.lod),
            load.is_some(),
            self.consecutive_mesh_dispatches,
        );
        match choice {
            Dispatch::Load => {
                if let Some(mesh) = mesh {
                    if mesh.lod == LodLevel::Lod0 {
                        self.metrics.fairness_load_dispatches += 1;
                    }
                    self.mesh_queue.push(mesh);
                }
                match load {
                    Some(load) => self.dispatch_load(load),
                    None => Ok(()),
                }
            }
            Dispatch::Mesh => {
                if let Some(load) = load {
                    self.load_queue.push(load);
                }
                match mesh {
                    Some(mesh) => self.dispatch_mesh(mesh),
                    None => Ok(()),
                }
            }
            Dispatch::Idle => Ok(()),
        }
    }

    fn dispatch_load(&mut self, entry: LoadQueueEntry) -> Result<(), RuntimeError> {
        if self.resident_payload_count() + self.reserved_load_slots.len()
            >= self.config.hard_resident_cap
        {
            self.metrics.hard_cap_blocks += 1;
            self.load_queue.push(entry);
            return Ok(());
        }
        self.reserved_load_slots.insert(entry.token);
        if let Some(record) = self.records.get_mut(&entry.coord) {
            record.residency = ResidencyState::Loading;
        }
        let job = WorkerJob::Load {
            coord: entry.coord,
            token: entry.token,
        };
        if let Err(job) = self.worker.try_dispatch(job) {
            self.reserved_load_slots.remove(&entry.token);
            if let Some(record) = self.records.get_mut(&entry.coord) {
                record.residency = ResidencyState::LoadQueued;
            }
            self.load_queue.push(entry);
            drop(job);
            return Ok(());
        }
        self.in_flight = Some(InFlight::Load);
        self.consecutive_mesh_dispatches = 0;
        self.metrics.load_jobs_dispatched += 1;
        Ok(())
    }

    fn dispatch_mesh(&mut self, entry: MeshQueueEntry) -> Result<(), RuntimeError> {
        let Some(stamp) = self.current_mesh_stamp(entry.coord) else {
            return Ok(());
        };
        let build_started = Instant::now();
        let Some((snapshot, derivation)) = self.snapshot(stamp) else {
            return Ok(());
        };
        self.metrics.snapshot_build.record(build_started.elapsed());
        if stamp.lod == LodLevel::Lod1 || derivation > Duration::ZERO {
            self.metrics.lod1_derivation.record(derivation);
        }
        let snapshot_bytes = snapshot.payload_bytes();
        let job = WorkerJob::Mesh(Box::new(MeshJob { stamp, snapshot }));
        if let Err(job) = self.worker.try_dispatch(job) {
            self.mesh_queue.push(entry);
            drop(job);
            return Ok(());
        }
        if let Some(record) = self.records.get_mut(&entry.coord) {
            record.mesh = MeshState::Meshing(stamp);
        }
        self.in_flight = Some(InFlight::Mesh);
        self.consecutive_mesh_dispatches = self.consecutive_mesh_dispatches.saturating_add(1);
        self.metrics.mesh_jobs_dispatched += 1;
        self.metrics.snapshot_bytes_dispatched += snapshot_bytes as u64;
        Ok(())
    }

    fn pop_valid_load(&mut self) -> Option<LoadQueueEntry> {
        while let Some(entry) = self.load_queue.pop() {
            let valid = matches!(
                self.records.get(&entry.coord),
                Some(ChunkRecord {
                    token,
                    residency: ResidencyState::LoadQueued,
                    ..
                }) if *token == entry.token && self.demand.dependency.contains(&entry.coord)
            );
            if valid {
                return Some(entry);
            }
        }
        None
    }

    fn pop_valid_mesh(&mut self) -> Option<MeshQueueEntry> {
        while let Some(entry) = self.mesh_queue.pop() {
            let valid = matches!(
                self.records.get(&entry.coord),
                Some(ChunkRecord {
                    token,
                    mesh: MeshState::Dirty(stamp),
                    ..
                }) if *token == entry.token && stamp.mesh_generation == entry.mesh_generation
            );
            if valid {
                return Some(entry);
            }
        }
        None
    }

    fn mesh_priority(&self, coord: ChunkCoord, lod: LodLevel) -> Reverse<(u8, u128, ChunkCoord)> {
        let Reverse((squared, coord)) = self.priority(coord);
        let rank = match lod {
            LodLevel::Lod0 => 0,
            LodLevel::Lod1 => 1,
        };
        Reverse((rank, squared, coord))
    }

    fn priority(&self, coord: ChunkCoord) -> Reverse<(u128, ChunkCoord)> {
        let squared = axis_distance(coord.x, self.center.x)
            + axis_distance(coord.y, self.center.y)
            + axis_distance(coord.z, self.center.z);
        Reverse((squared, coord))
    }
}

/// Either slab type, so one timing closure can wrap both derivations.
enum MeshSnapshotSlab {
    Fine(FaceSlab<CHUNK_EDGE>),
    Coarse(FaceSlab<COARSE_EDGE>),
}

fn chebyshev_distance(coord: ChunkCoord, center: ChunkCoord) -> u32 {
    let axis = |left: i32, right: i32| (i64::from(left) - i64::from(right)).unsigned_abs();
    let largest = axis(coord.x, center.x)
        .max(axis(coord.y, center.y))
        .max(axis(coord.z, center.z));
    u32::try_from(largest).unwrap_or(u32::MAX)
}

fn axis_distance(left: i32, right: i32) -> u128 {
    let difference = i128::from(left) - i128::from(right);
    difference.unsigned_abs().pow(2)
}

const fn residency_status_of(record: &ChunkRecord) -> ResidencyStatus {
    match record.residency {
        ResidencyState::LoadQueued => ResidencyStatus::LoadQueued,
        ResidencyState::Loading => ResidencyStatus::Loading,
        ResidencyState::CpuResident { .. } => ResidencyStatus::CpuResident,
        ResidencyState::KnownAbsent => ResidencyStatus::KnownAbsent,
        ResidencyState::EvictPending(_) => ResidencyStatus::EvictPending,
    }
}

const fn mesh_status_of(record: &ChunkRecord) -> MeshStatus {
    match record.mesh {
        MeshState::NotRequired => MeshStatus::NotRequired,
        MeshState::WaitingForNeighbors => MeshStatus::WaitingForNeighbors,
        MeshState::Dirty(_) => MeshStatus::Dirty,
        MeshState::Meshing(_) => MeshStatus::Meshing,
        MeshState::CpuReady { .. } => MeshStatus::CpuReady,
        MeshState::Committed => MeshStatus::Committed,
    }
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use super::*;

    fn runtime_with(config: StreamingConfig) -> Result<StreamingRuntime, RuntimeError> {
        StreamingRuntime::new(config, ChunkCoord::default())
    }

    fn force_load(
        runtime: &mut StreamingRuntime,
        coord: ChunkCoord,
        source: SourceChunk,
    ) -> Result<RequestToken, RuntimeError> {
        let token = runtime
            .request_token(coord)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        let record = runtime
            .records
            .get_mut(&coord)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        record.residency = ResidencyState::Loading;
        runtime.reserved_load_slots.insert(token);
        runtime.integrate_load(coord, token, source)?;
        Ok(token)
    }

    fn load_center_neighborhood(runtime: &mut StreamingRuntime) -> Result<(), RuntimeError> {
        let center = ChunkCoord::default();
        force_load(runtime, center, DiagnosticChunkSource.load(center))?;
        for face in Face::ALL {
            let coord = center
                .neighbor(face)
                .ok_or(RuntimeError::WorkerDisconnected)?;
            force_load(runtime, coord, DiagnosticChunkSource.load(coord))?;
        }
        Ok(())
    }

    #[test]
    fn teleport_never_exceeds_hard_cap() -> Result<(), RuntimeError> {
        let config = StreamingConfig {
            hard_resident_cap: 3,
            ..StreamingConfig::default()
        };
        let mut runtime = runtime_with(config)?;
        for _ in 0..200 {
            runtime.poll()?;
            assert!(runtime.resident_payload_count() + runtime.reserved_load_count() <= 3);
            thread::yield_now();
        }
        runtime.set_demand_center(ChunkCoord::new(-20, 0, 20))?;
        for _ in 0..200 {
            runtime.poll()?;
            assert!(runtime.resident_payload_count() + runtime.reserved_load_count() <= 3);
            thread::yield_now();
        }
        Ok(())
    }

    #[test]
    fn load_reserves_hard_cap_capacity_before_dispatch() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig {
            hard_resident_cap: 1,
            ..StreamingConfig::default()
        })?;
        runtime.poll()?;
        assert_eq!(runtime.resident_payload_count(), 0);
        assert_eq!(runtime.reserved_load_count(), 1);
        assert!(matches!(runtime.in_flight, Some(InFlight::Load)));
        Ok(())
    }

    #[test]
    fn boundary_oscillation_preserves_retained_incarnations() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        let origin = ChunkCoord::default();
        let token = runtime
            .request_token(origin)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        runtime.set_demand_center(ChunkCoord::new(-1, 0, 0))?;
        runtime.set_demand_center(origin)?;
        assert_eq!(runtime.request_token(origin), Some(token));
        assert_eq!(runtime.tracked_count(), runtime.demand.dependency.len());
        Ok(())
    }

    #[test]
    fn dependency_only_chunk_keeps_cpu_payload_without_mesh() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        load_center_neighborhood(&mut runtime)?;
        let origin = ChunkCoord::default();
        let token = runtime
            .request_token(origin)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        runtime.set_demand_center(ChunkCoord::new(2, 0, 0))?;
        assert_eq!(runtime.request_token(origin), Some(token));
        assert_eq!(
            runtime.residency_status(origin),
            Some(ResidencyStatus::CpuResident)
        );
        assert_eq!(runtime.mesh_status(origin), Some(MeshStatus::NotRequired));
        Ok(())
    }

    #[test]
    fn eviction_and_reentry_uses_a_new_token_and_rejects_aba_result() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        let coord = ChunkCoord::default();
        let old = runtime
            .request_token(coord)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        runtime
            .records
            .get_mut(&coord)
            .ok_or(RuntimeError::WorkerDisconnected)?
            .residency = ResidencyState::Loading;
        runtime.reserved_load_slots.insert(old);
        runtime.set_demand_center(ChunkCoord::new(20, 0, 0))?;
        runtime.set_demand_center(coord)?;
        let new = runtime
            .request_token(coord)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        assert_ne!(old, new);
        runtime.integrate_load(coord, old, SourceChunk::Present(Chunk::empty()))?;
        assert_eq!(runtime.metrics.stale_load_results, 1);
        assert_eq!(runtime.reserved_load_count(), 0);
        assert_eq!(runtime.request_token(coord), Some(new));
        Ok(())
    }

    #[test]
    fn stale_load_token_is_rejected() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        let coord = ChunkCoord::default();
        let current = runtime
            .request_token(coord)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        runtime.integrate_load(
            coord,
            RequestToken(current.0 + 10_000),
            SourceChunk::Present(Chunk::empty()),
        )?;
        assert_eq!(runtime.metrics.stale_load_results, 1);
        assert_eq!(
            runtime.residency_status(coord),
            Some(ResidencyStatus::LoadQueued)
        );
        Ok(())
    }

    #[test]
    fn known_absent_unblocks_mesh_but_unavailable_neighbor_does_not() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        let center = ChunkCoord::default();
        force_load(&mut runtime, center, SourceChunk::Present(Chunk::empty()))?;
        assert_eq!(
            runtime.mesh_status(center),
            Some(MeshStatus::WaitingForNeighbors)
        );
        for face in Face::ALL {
            let coord = center
                .neighbor(face)
                .ok_or(RuntimeError::WorkerDisconnected)?;
            force_load(&mut runtime, coord, SourceChunk::KnownAbsent)?;
        }
        assert_eq!(runtime.mesh_status(center), Some(MeshStatus::Dirty));
        Ok(())
    }

    #[test]
    fn known_absent_render_chunk_never_reports_a_pending_mesh() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        let center = ChunkCoord::default();
        assert_eq!(
            runtime.mesh_status(center),
            Some(MeshStatus::WaitingForNeighbors),
            "a render chunk still loading is waiting for data"
        );
        force_load(&mut runtime, center, SourceChunk::KnownAbsent)?;
        assert_eq!(runtime.mesh_status(center), Some(MeshStatus::NotRequired));
        assert_eq!(runtime.summary().mesh_waiting, 26);
        runtime.set_demand_center(ChunkCoord::new(1, 0, 0))?;
        runtime.set_demand_center(center)?;
        assert_eq!(runtime.mesh_status(center), Some(MeshStatus::NotRequired));
        Ok(())
    }

    #[test]
    fn neighbor_arrival_invalidates_current_mesh_immediately() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        load_center_neighborhood(&mut runtime)?;
        let center = ChunkCoord::default();
        let original = runtime
            .current_mesh_stamp(center)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        let neighbor = center
            .neighbor(Face::PositiveX)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        let record = runtime
            .records
            .get_mut(&neighbor)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        let ResidencyState::CpuResident {
            content_generation, ..
        } = &mut record.residency
        else {
            return Err(RuntimeError::WorkerDisconnected);
        };
        *content_generation += 1;
        runtime.invalidate_neighbors(neighbor, InvalidationCause::Data)?;
        let replacement = runtime
            .current_mesh_stamp(center)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        assert_ne!(original.neighbors, replacement.neighbors);
        assert_eq!(runtime.mesh_status(center), Some(MeshStatus::Dirty));
        Ok(())
    }

    #[test]
    fn stale_center_mesh_generation_is_rejected() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        load_center_neighborhood(&mut runtime)?;
        let center = ChunkCoord::default();
        let stale = runtime
            .current_mesh_stamp(center)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        runtime
            .records
            .get_mut(&center)
            .ok_or(RuntimeError::WorkerDisconnected)?
            .mesh = MeshState::Meshing(stale);
        runtime.invalidate_mesh(center, InvalidationCause::Data)?;
        runtime.integrate_mesh(stale, Mesh::default())?;
        assert_eq!(runtime.metrics.stale_mesh_results, 1);
        Ok(())
    }

    #[test]
    fn stale_neighbor_generation_is_rejected() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        load_center_neighborhood(&mut runtime)?;
        let center = ChunkCoord::default();
        let stale = runtime
            .current_mesh_stamp(center)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        runtime
            .records
            .get_mut(&center)
            .ok_or(RuntimeError::WorkerDisconnected)?
            .mesh = MeshState::Meshing(stale);
        let neighbor = center
            .neighbor(Face::NegativeZ)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        let record = runtime
            .records
            .get_mut(&neighbor)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        let ResidencyState::CpuResident {
            content_generation, ..
        } = &mut record.residency
        else {
            return Err(RuntimeError::WorkerDisconnected);
        };
        *content_generation += 1;
        runtime.integrate_mesh(stale, Mesh::default())?;
        assert_eq!(runtime.metrics.stale_mesh_results, 1);
        Ok(())
    }

    #[test]
    fn unload_during_mesh_job_discards_result() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        load_center_neighborhood(&mut runtime)?;
        let center = ChunkCoord::default();
        let stamp = runtime
            .current_mesh_stamp(center)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        runtime
            .records
            .get_mut(&center)
            .ok_or(RuntimeError::WorkerDisconnected)?
            .mesh = MeshState::Meshing(stamp);
        runtime.set_demand_center(ChunkCoord::new(20, 0, 0))?;
        runtime.integrate_mesh(stamp, Mesh::default())?;
        assert_eq!(runtime.metrics.stale_mesh_results, 1);
        Ok(())
    }

    #[test]
    fn scheduler_forces_load_after_four_meshes() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        let load = runtime
            .pop_valid_load()
            .ok_or(RuntimeError::WorkerDisconnected)?;
        runtime.load_queue.push(load);
        runtime.consecutive_mesh_dispatches = MESH_BURST_BEFORE_LOAD;
        runtime.mesh_queue.push(MeshQueueEntry {
            priority: runtime.mesh_priority(load.coord, LodLevel::Lod0),
            coord: load.coord,
            token: load.token,
            mesh_generation: 0,
            lod: LodLevel::Lod0,
        });
        // A synthetic valid Dirty state isolates the deterministic selector.
        let stamp = MeshStamp {
            coord: load.coord,
            request_token: load.token,
            lod: LodLevel::Lod0,
            mesh_generation: 0,
            center_content_generation: 1,
            neighbors: [NeighborStamp::KnownAbsent {
                coord: ChunkCoord::default(),
                token: load.token,
            }; 6],
        };
        let record = runtime
            .records
            .get_mut(&load.coord)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        record.mesh = MeshState::Dirty(stamp);
        runtime.dispatch_one()?;
        assert_eq!(runtime.metrics.fairness_load_dispatches, 1);
        assert!(matches!(runtime.in_flight, Some(InFlight::Load)));
        Ok(())
    }

    #[test]
    fn eviction_finalization_is_bounded_per_update() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig {
            max_cpu_evictions_per_update: 2,
            ..StreamingConfig::default()
        })?;
        load_center_neighborhood(&mut runtime)?;
        runtime.set_demand_center(ChunkCoord::new(40, 0, 40))?;
        let backlog = runtime.eviction_backlog();
        assert!(backlog > 2, "teleport should retire more than one update");

        runtime.finalize_evictions();
        assert_eq!(runtime.eviction_backlog(), backlog - 2);
        assert_eq!(runtime.metrics.cpu_evictions_finalized, 2);
        assert_eq!(runtime.metrics.eviction_budget_hits, 1);
        Ok(())
    }

    #[test]
    fn a_full_eviction_backlog_blocks_loads_until_it_is_released() -> Result<(), RuntimeError> {
        let cap = 7;
        let mut runtime = runtime_with(StreamingConfig {
            render_radius: 0,
            dependency_halo: 1,
            retention_radius: 1,
            hard_resident_cap: cap,
            max_cpu_evictions_per_update: 1,
            ..StreamingConfig::default()
        })?;
        load_center_neighborhood(&mut runtime)?;
        assert_eq!(runtime.resident_payload_count(), cap);

        runtime.set_demand_center(ChunkCoord::new(-64, 0, 64))?;
        assert_eq!(runtime.eviction_backlog(), cap);
        assert_eq!(
            runtime.resident_payload_count(),
            cap,
            "retired payloads must keep counting against the cap"
        );

        // Nothing has been released yet, so the very first load is refused.
        runtime.dispatch_one()?;
        assert_eq!(runtime.metrics.hard_cap_blocks, 1);
        assert!(runtime.in_flight.is_none());
        assert_eq!(runtime.reserved_load_count(), 0);

        // One bounded release frees exactly one slot.
        runtime.finalize_evictions();
        assert_eq!(runtime.eviction_backlog(), cap - 1);
        assert_eq!(runtime.metrics.cpu_evictions_finalized, 1);
        assert_eq!(runtime.metrics.eviction_budget_hits, 1);
        runtime.dispatch_one()?;
        assert!(matches!(runtime.in_flight, Some(InFlight::Load)));
        assert_eq!(
            runtime.resident_payload_count() + runtime.reserved_load_count(),
            cap
        );
        Ok(())
    }

    #[test]
    fn teleport_respects_the_hard_cap_while_an_eviction_backlog_drains() -> Result<(), RuntimeError>
    {
        let cap = 7;
        let mut runtime = runtime_with(StreamingConfig {
            render_radius: 0,
            dependency_halo: 1,
            retention_radius: 1,
            hard_resident_cap: cap,
            max_cpu_evictions_per_update: 1,
            ..StreamingConfig::default()
        })?;
        for _ in 0..10_000 {
            runtime.poll()?;
            if runtime.is_idle() {
                break;
            }
            thread::sleep(Duration::from_micros(50));
        }
        assert!(runtime.is_idle());
        assert_eq!(runtime.resident_payload_count(), cap);

        // Teleport into a different in-bounds area so replacement loads carry
        // payloads and compete with the retiring ones for the same cap.
        runtime.set_demand_center(ChunkCoord::new(3, 0, -3))?;
        assert_eq!(
            runtime.eviction_backlog(),
            cap,
            "teleport should retire every previous payload"
        );
        for _ in 0..10_000 {
            runtime.poll()?;
            assert!(
                runtime.resident_payload_count() + runtime.reserved_load_count() <= cap,
                "hard cap breached while draining backlog: resident={} reserved={} backlog={}",
                runtime.resident_payload_count(),
                runtime.reserved_load_count(),
                runtime.eviction_backlog()
            );
            if runtime.is_idle() && runtime.eviction_backlog() == 0 {
                break;
            }
            thread::sleep(Duration::from_micros(50));
        }
        assert_eq!(runtime.eviction_backlog(), 0);
        assert!(runtime.is_idle());
        assert_eq!(runtime.metrics.cpu_evictions_finalized, cap as u64);
        assert!(runtime.metrics.eviction_budget_hits > 0);
        assert_eq!(runtime.resident_payload_count(), cap);
        assert_eq!(runtime.tracked_count(), cap);
        Ok(())
    }

    #[test]
    fn retention_only_chunks_are_never_offered_for_rendering() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        load_center_neighborhood(&mut runtime)?;
        for (coord, _, _) in runtime.render_ready_meshes() {
            assert!(runtime.demand.render.contains(&coord));
        }
        let summary = runtime.summary();
        assert_eq!(summary.tracked, runtime.records.len());
        assert_eq!(summary.cpu_resident, runtime.resident_payload_count());
        assert_eq!(
            summary.resident_payload_bytes,
            summary.cpu_resident * CHUNK_BYTES
        );
        Ok(())
    }

    fn m3c() -> Result<StreamingRuntime, RuntimeError> {
        runtime_with(StreamingConfig::m3c_diagnostic())
    }

    fn lod(runtime: &StreamingRuntime, x: i32, y: i32, z: i32) -> Option<LodLevel> {
        runtime.desired_lod(ChunkCoord::new(x, y, z))
    }

    #[test]
    fn lod0_only_profile_never_selects_lod1() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        runtime.set_demand_center(ChunkCoord::new(5, 0, -5))?;
        let summary = runtime.summary();
        assert_eq!(summary.lod0_desired, 27);
        assert_eq!(summary.lod1_desired, 0);
        assert_eq!(runtime.metrics.lod_swaps, 0);
        Ok(())
    }

    #[test]
    fn banded_startup_assigns_levels_deterministically() -> Result<(), RuntimeError> {
        let runtime = m3c()?;
        let summary = runtime.summary();
        assert_eq!(summary.lod0_desired, 27);
        assert_eq!(
            summary.lod1_desired, 316,
            "98 band chunks without history plus 218"
        );
        assert_eq!(lod(&runtime, 1, 1, 1), Some(LodLevel::Lod0));
        assert_eq!(lod(&runtime, 2, 0, 0), Some(LodLevel::Lod1));
        assert_eq!(lod(&runtime, 3, 0, 0), Some(LodLevel::Lod1));
        assert_eq!(
            lod(&runtime, 4, 0, 0),
            Some(LodLevel::Lod1),
            "dependency halo starts coarse"
        );
        Ok(())
    }

    #[test]
    fn approach_promotes_only_inside_radius_one() -> Result<(), RuntimeError> {
        let mut runtime = m3c()?;
        assert_eq!(lod(&runtime, 3, 0, 0), Some(LodLevel::Lod1));
        runtime.set_demand_center(ChunkCoord::new(1, 0, 0))?;
        assert_eq!(
            lod(&runtime, 3, 0, 0),
            Some(LodLevel::Lod1),
            "band keeps Lod1"
        );
        runtime.set_demand_center(ChunkCoord::new(2, 0, 0))?;
        assert_eq!(lod(&runtime, 3, 0, 0), Some(LodLevel::Lod0));
        Ok(())
    }

    #[test]
    fn retreat_keeps_lod0_through_the_band() -> Result<(), RuntimeError> {
        let mut runtime = m3c()?;
        runtime.set_demand_center(ChunkCoord::new(2, 0, 0))?;
        assert_eq!(lod(&runtime, 3, 0, 0), Some(LodLevel::Lod0));
        runtime.set_demand_center(ChunkCoord::new(1, 0, 0))?;
        assert_eq!(
            lod(&runtime, 3, 0, 0),
            Some(LodLevel::Lod0),
            "band keeps Lod0"
        );
        runtime.set_demand_center(ChunkCoord::new(0, 0, 0))?;
        assert_eq!(lod(&runtime, 3, 0, 0), Some(LodLevel::Lod1));
        Ok(())
    }

    #[test]
    fn oscillation_across_one_boundary_never_swaps_again() -> Result<(), RuntimeError> {
        let mut runtime = m3c()?;
        runtime.set_demand_center(ChunkCoord::new(1, 0, 0))?;
        runtime.set_demand_center(ChunkCoord::new(0, 0, 0))?;
        let settled = runtime.metrics.lod_swaps;
        assert!(settled > 0, "the first crossing promotes band chunks");
        for _ in 0..4 {
            runtime.set_demand_center(ChunkCoord::new(1, 0, 0))?;
            runtime.set_demand_center(ChunkCoord::new(0, 0, 0))?;
        }
        assert_eq!(runtime.metrics.lod_swaps, settled, "no ping-pong");
        assert_eq!(
            lod(&runtime, 2, 0, 0),
            Some(LodLevel::Lod0),
            "history survives"
        );
        Ok(())
    }

    #[test]
    fn teleport_assigns_levels_by_distance_only() -> Result<(), RuntimeError> {
        let mut runtime = m3c()?;
        runtime.set_demand_center(ChunkCoord::new(40, 0, -40))?;
        assert_eq!(lod(&runtime, 40, 0, -40), Some(LodLevel::Lod0));
        assert_eq!(lod(&runtime, 41, 1, -41), Some(LodLevel::Lod0));
        assert_eq!(
            lod(&runtime, 42, 0, -40),
            Some(LodLevel::Lod1),
            "band without history"
        );
        assert_eq!(lod(&runtime, 43, 0, -40), Some(LodLevel::Lod1));
        assert_eq!(lod(&runtime, 2, 0, 0), None, "old records retired");
        Ok(())
    }

    #[test]
    fn eviction_and_reentry_forget_lod_history() -> Result<(), RuntimeError> {
        let mut runtime = m3c()?;
        runtime.set_demand_center(ChunkCoord::new(1, 0, 0))?;
        runtime.set_demand_center(ChunkCoord::new(0, 0, 0))?;
        let coord = ChunkCoord::new(2, 0, 0);
        assert_eq!(runtime.desired_lod(coord), Some(LodLevel::Lod0));
        let old_token = runtime.request_token(coord);
        runtime.set_demand_center(ChunkCoord::new(40, 0, -40))?;
        for _ in 0..200 {
            runtime.finalize_evictions();
        }
        runtime.set_demand_center(ChunkCoord::new(0, 0, 0))?;
        assert_eq!(
            runtime.desired_lod(coord),
            Some(LodLevel::Lod1),
            "fresh decision"
        );
        assert_ne!(runtime.request_token(coord), old_token);
        Ok(())
    }

    #[test]
    fn old_level_result_is_rejected_and_tokens_survive_a_lod_change() -> Result<(), RuntimeError> {
        let mut runtime = m3c()?;
        load_center_neighborhood(&mut runtime)?;
        let center = ChunkCoord::default();
        let token = runtime.request_token(center);
        let stale = runtime
            .current_mesh_stamp(center)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        assert_eq!(stale.lod, LodLevel::Lod0);
        runtime
            .records
            .get_mut(&center)
            .ok_or(RuntimeError::WorkerDisconnected)?
            .mesh = MeshState::Meshing(stale);

        runtime.set_demand_center(ChunkCoord::new(3, 0, 0))?;
        assert_eq!(runtime.desired_lod(center), Some(LodLevel::Lod1));
        assert_eq!(runtime.request_token(center), token, "token untouched");
        let ResidencyState::CpuResident {
            content_generation, ..
        } = runtime
            .records
            .get(&center)
            .ok_or(RuntimeError::WorkerDisconnected)?
            .residency
        else {
            return Err(RuntimeError::WorkerDisconnected);
        };
        assert_eq!(content_generation, stale.center_content_generation);

        runtime.integrate_mesh(stale, Mesh::default())?;
        assert_eq!(runtime.metrics.stale_mesh_results, 1);
        assert_eq!(runtime.metrics.stale_lod_results, 1);
        assert_ne!(runtime.mesh_status(center), Some(MeshStatus::CpuReady));
        Ok(())
    }

    #[test]
    fn neighbor_level_change_invalidates_the_seam() -> Result<(), RuntimeError> {
        let mut runtime = m3c()?;
        load_center_neighborhood(&mut runtime)?;
        let center = ChunkCoord::default();
        let before = runtime
            .current_mesh_stamp(center)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        assert!(before.neighbors.iter().all(|stamp| matches!(
            stamp,
            NeighborStamp::Resident {
                seam: SeamContract::Same,
                ..
            }
        )));
        runtime.set_demand_center(ChunkCoord::new(-2, 0, 0))?;
        assert_eq!(
            runtime.desired_lod(center),
            Some(LodLevel::Lod0),
            "band keeps Lod0"
        );
        assert_eq!(lod(&runtime, 1, 0, 0), Some(LodLevel::Lod1));
        let after = runtime
            .current_mesh_stamp(center)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        assert_ne!(before, after);
        assert!(matches!(
            after.neighbors[Face::PositiveX as usize],
            NeighborStamp::Resident {
                seam: SeamContract::CoarseNeighbor,
                ..
            }
        ));
        assert!(after.mesh_generation > before.mesh_generation);
        assert_eq!(runtime.mesh_status(center), Some(MeshStatus::Dirty));
        Ok(())
    }

    #[test]
    fn dependency_only_neighbors_are_content_only() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        let edge = ChunkCoord::new(1, 0, 0);
        let halo = ChunkCoord::new(2, 0, 0);
        force_load(&mut runtime, edge, SourceChunk::Present(Chunk::empty()))?;
        force_load(&mut runtime, halo, SourceChunk::Present(Chunk::empty()))?;
        let stamp = runtime
            .neighbor_stamp(edge, Face::PositiveX)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        assert!(matches!(
            stamp,
            NeighborStamp::Resident {
                seam: SeamContract::Same,
                ..
            }
        ));
        Ok(())
    }

    fn drain_one_result(runtime: &mut StreamingRuntime) -> Result<(), RuntimeError> {
        for _ in 0..20_000 {
            if runtime
                .worker
                .try_result()
                .map_err(|()| RuntimeError::WorkerDisconnected)?
                .is_some()
            {
                runtime.in_flight = None;
                return Ok(());
            }
            thread::sleep(Duration::from_micros(50));
        }
        Err(RuntimeError::WorkerDisconnected)
    }

    fn load_lod1_neighborhood(runtime: &mut StreamingRuntime) -> Result<ChunkCoord, RuntimeError> {
        let coarse = ChunkCoord::new(3, 0, 0);
        force_load(runtime, coarse, DiagnosticChunkSource.load(coarse))?;
        for face in Face::ALL {
            let coord = coarse
                .neighbor(face)
                .ok_or(RuntimeError::WorkerDisconnected)?;
            if runtime.residency_status(coord) != Some(ResidencyStatus::CpuResident) {
                force_load(runtime, coord, DiagnosticChunkSource.load(coord))?;
            }
        }
        Ok(coarse)
    }

    #[test]
    fn dispatch_choice_orders_lod0_then_load_then_lod1() {
        assert_eq!(
            choose_dispatch(Some(LodLevel::Lod0), true, 0),
            Dispatch::Mesh
        );
        assert_eq!(
            choose_dispatch(Some(LodLevel::Lod0), false, 9),
            Dispatch::Mesh
        );
        assert_eq!(
            choose_dispatch(Some(LodLevel::Lod0), true, MESH_BURST_BEFORE_LOAD),
            Dispatch::Load,
            "fairness after four consecutive meshes"
        );
        assert_eq!(
            choose_dispatch(Some(LodLevel::Lod1), true, 0),
            Dispatch::Load
        );
        assert_eq!(
            choose_dispatch(Some(LodLevel::Lod1), false, 0),
            Dispatch::Mesh
        );
        assert_eq!(choose_dispatch(None, true, 0), Dispatch::Load);
        assert_eq!(choose_dispatch(None, false, 0), Dispatch::Idle);
    }

    #[test]
    fn scheduler_dispatches_lod0_mesh_then_load_then_lod1_mesh() -> Result<(), RuntimeError> {
        let mut runtime = m3c()?;
        load_center_neighborhood(&mut runtime)?;
        let coarse = load_lod1_neighborhood(&mut runtime)?;
        let center = ChunkCoord::default();
        assert_eq!(runtime.mesh_status(center), Some(MeshStatus::Dirty));
        assert_eq!(runtime.mesh_status(coarse), Some(MeshStatus::Dirty));
        assert_eq!(runtime.desired_lod(coarse), Some(LodLevel::Lod1));
        assert!(runtime.summary().load_queued > 0, "loads are ready too");
        // The Lod1 entry is nearer in the heap only by level; distance alone
        // would have picked it (0 vs 9) if the level rank were ignored.

        runtime.dispatch_one()?;
        assert!(matches!(runtime.in_flight, Some(InFlight::Mesh)));
        assert_eq!(runtime.mesh_status(center), Some(MeshStatus::Meshing));
        assert_eq!(runtime.mesh_status(coarse), Some(MeshStatus::Dirty));
        drain_one_result(&mut runtime)?;

        let loads = runtime.metrics.load_jobs_dispatched;
        runtime.dispatch_one()?;
        assert!(matches!(runtime.in_flight, Some(InFlight::Load)));
        assert_eq!(runtime.metrics.load_jobs_dispatched, loads + 1);
        assert_eq!(
            runtime.mesh_status(coarse),
            Some(MeshStatus::Dirty),
            "Lod1 waits"
        );
        drain_one_result(&mut runtime)?;

        runtime.load_queue.clear();
        runtime.dispatch_one()?;
        assert!(matches!(runtime.in_flight, Some(InFlight::Mesh)));
        assert_eq!(runtime.mesh_status(coarse), Some(MeshStatus::Meshing));
        assert_eq!(runtime.metrics.lod1_derivation.count, 1);
        assert_eq!(runtime.metrics.snapshot_build.count, 2);
        drain_one_result(&mut runtime)?;
        Ok(())
    }

    #[test]
    fn load_beats_a_lod1_mesh_when_no_lod0_mesh_is_ready() -> Result<(), RuntimeError> {
        let mut runtime = m3c()?;
        let coarse = load_lod1_neighborhood(&mut runtime)?;
        assert_eq!(runtime.mesh_status(coarse), Some(MeshStatus::Dirty));
        assert_eq!(
            runtime.mesh_status(ChunkCoord::default()),
            Some(MeshStatus::WaitingForNeighbors)
        );
        runtime.dispatch_one()?;
        assert!(matches!(runtime.in_flight, Some(InFlight::Load)));
        assert_eq!(runtime.mesh_status(coarse), Some(MeshStatus::Dirty));
        assert_eq!(
            runtime.metrics.fairness_load_dispatches, 0,
            "not a fairness event"
        );
        drain_one_result(&mut runtime)?;
        Ok(())
    }

    #[test]
    fn neighbor_presentation_change_counts_as_lod_stale() -> Result<(), RuntimeError> {
        let mut runtime = m3c()?;
        load_center_neighborhood(&mut runtime)?;
        let center = ChunkCoord::default();
        let stale = runtime
            .current_mesh_stamp(center)
            .ok_or(RuntimeError::WorkerDisconnected)?;
        runtime
            .records
            .get_mut(&center)
            .ok_or(RuntimeError::WorkerDisconnected)?
            .mesh = MeshState::Meshing(stale);
        // Center stays Lod0 in the band; its +X neighbor becomes Lod1.
        runtime.set_demand_center(ChunkCoord::new(-2, 0, 0))?;
        assert_eq!(runtime.desired_lod(center), Some(LodLevel::Lod0));
        runtime.integrate_mesh(stale, Mesh::default())?;
        assert_eq!(runtime.metrics.stale_mesh_results, 1);
        assert_eq!(runtime.metrics.stale_lod_results, 1);
        Ok(())
    }

    #[test]
    fn m3c_profile_reaches_idle_with_meshes_at_both_levels() -> Result<(), RuntimeError> {
        let mut runtime = m3c()?;
        for _ in 0..200_000 {
            runtime.poll()?;
            if runtime.is_idle() {
                break;
            }
            thread::sleep(Duration::from_micros(50));
        }
        assert!(runtime.is_idle());
        let summary = runtime.summary();
        assert_eq!(summary.lod0_desired + summary.lod1_desired, 343);
        assert!(summary.lod0_ready > 0);
        assert!(summary.lod1_ready > 0);
        assert_eq!(summary.lod0_ready + summary.lod1_ready, summary.mesh_ready);
        assert_eq!(runtime.metrics.stale_lod_results, 0);
        assert!(runtime.resident_payload_count() <= 810);
        let metrics = runtime.metrics();
        assert_eq!(metrics.snapshot_build.count, metrics.mesh_jobs_dispatched);
        assert_eq!(
            metrics.worker_mesh_lod0.count + metrics.worker_mesh_lod1.count,
            metrics.accepted_mesh_results + metrics.stale_mesh_results
        );
        assert!(metrics.lod1_derivation.count >= u64::try_from(summary.lod1_ready).unwrap_or(0));
        Ok(())
    }

    #[test]
    fn request_token_overflow_is_fatal_and_never_wraps() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig::default())?;
        runtime.next_token = u64::MAX;
        assert!(matches!(
            runtime.allocate_token(),
            Err(RuntimeError::RequestTokenExhausted)
        ));
        assert_eq!(runtime.next_token, u64::MAX);
        Ok(())
    }

    /// A group is the unit of seam coherence: one member that is not ready for
    /// its current target commits nothing, so no drawn pair can end up mixing
    /// an old and a new seam.
    #[test]
    fn a_group_with_one_unready_member_commits_nothing() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig {
            render_radius: 0,
            dependency_halo: 1,
            retention_radius: 1,
            hard_resident_cap: 16,
            ..StreamingConfig::default()
        })?;
        for _ in 0..10_000 {
            runtime.poll()?;
            if runtime.is_idle() {
                break;
            }
            thread::sleep(Duration::from_micros(50));
        }
        let center = ChunkCoord::default();
        assert_eq!(runtime.mesh_status(center), Some(MeshStatus::CpuReady));
        // A dependency-only neighbor is never meshed, so it can never be ready.
        let neighbor = ChunkCoord::new(1, 0, 0);
        assert_eq!(runtime.mesh_status(neighbor), Some(MeshStatus::NotRequired));
        assert!(!runtime.group_is_ready(&[center, neighbor]));

        assert_eq!(runtime.commit_group(&[center, neighbor]), 0);
        assert_eq!(
            runtime.mesh_status(center),
            Some(MeshStatus::CpuReady),
            "the ready member must not be committed on its own"
        );
        assert!(runtime.committed_mesh(center).is_none());

        let mut external_called = false;
        assert!(!runtime.group_is_ready(&[]));
        assert!(!runtime.group_is_ready(&[center, center]));
        assert!(!runtime.group_is_ready(&[center, neighbor, center]));
        let duplicate = runtime.commit_group_with(&[center, center], || {
            external_called = true;
            Ok::<(), ()>(())
        });
        assert_eq!(duplicate, Ok(0));
        assert!(
            !external_called,
            "invalid membership must be rejected before the external swap"
        );

        let refused = runtime.commit_group_with(&[center], || Err("presentation refused"));
        assert_eq!(refused, Err("presentation refused"));
        assert_eq!(runtime.mesh_status(center), Some(MeshStatus::CpuReady));
        assert!(runtime.committed_mesh(center).is_none());

        assert_eq!(runtime.commit_group(&[center]), 1);
        assert!(runtime.committed_mesh(center).is_some());
        Ok(())
    }

    #[test]
    fn real_worker_reaches_a_stable_bounded_state() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig {
            render_radius: 0,
            dependency_halo: 1,
            retention_radius: 1,
            hard_resident_cap: 16,
            ..StreamingConfig::default()
        })?;
        for _ in 0..10_000 {
            runtime.poll()?;
            if runtime.is_idle() {
                break;
            }
            thread::sleep(Duration::from_micros(50));
        }
        assert!(runtime.is_idle());
        assert_eq!(runtime.resident_payload_count(), 7);
        assert_eq!(runtime.reserved_load_count(), 0);
        assert_eq!(
            runtime.mesh_status(ChunkCoord::default()),
            Some(MeshStatus::CpuReady)
        );
        assert!(runtime.metrics.snapshot_bytes_dispatched <= 76 * 1_024);
        Ok(())
    }
}
