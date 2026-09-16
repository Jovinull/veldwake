use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    fmt,
};

use veldwake_voxel::{CHUNK_BYTES, Chunk, ChunkCoord, Face, FaceSlab, Mesh, OwnedMeshingSnapshot};

use crate::{
    demand::{DemandError, DemandSets, StreamingConfig},
    source::{DiagnosticChunkSource, SourceChunk},
    types::{MeshStamp, NeighborStamp, RequestToken},
    worker::{MeshJob, Worker, WorkerJob, WorkerResult},
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
    CpuReady { stamp: MeshStamp, mesh: Mesh },
}

#[derive(Debug)]
struct ChunkRecord {
    token: RequestToken,
    residency: ResidencyState,
    mesh_generation: u64,
    mesh: MeshState,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct LoadQueueEntry {
    priority: Reverse<(u128, ChunkCoord)>,
    coord: ChunkCoord,
    token: RequestToken,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct MeshQueueEntry {
    priority: Reverse<(u128, ChunkCoord)>,
    coord: ChunkCoord,
    token: RequestToken,
    mesh_generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InFlight {
    Load,
    Mesh,
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
            self.invalidate_neighbors(coord)?;
        }

        self.create_dependency_records()?;

        let tracked: Vec<_> = self.records.keys().copied().collect();
        for coord in tracked {
            let should_render = self.demand.render.contains(&coord);
            let currently_required = !matches!(
                self.records.get(&coord).map(|record| &record.mesh),
                Some(MeshState::NotRequired)
            );
            if should_render != currently_required {
                self.invalidate_mesh(coord)?;
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
    pub fn residency_status(&self, coord: ChunkCoord) -> Option<ResidencyStatus> {
        self.records
            .get(&coord)
            .map(|record| match record.residency {
                ResidencyState::LoadQueued => ResidencyStatus::LoadQueued,
                ResidencyState::Loading => ResidencyStatus::Loading,
                ResidencyState::CpuResident { .. } => ResidencyStatus::CpuResident,
                ResidencyState::KnownAbsent => ResidencyStatus::KnownAbsent,
                ResidencyState::EvictPending(_) => ResidencyStatus::EvictPending,
            })
    }

    #[must_use]
    pub fn mesh_status(&self, coord: ChunkCoord) -> Option<MeshStatus> {
        self.records.get(&coord).map(|record| match record.mesh {
            MeshState::NotRequired => MeshStatus::NotRequired,
            MeshState::WaitingForNeighbors => MeshStatus::WaitingForNeighbors,
            MeshState::Dirty(_) => MeshStatus::Dirty,
            MeshState::Meshing(_) => MeshStatus::Meshing,
            MeshState::CpuReady { .. } => MeshStatus::CpuReady,
        })
    }

    #[must_use]
    pub fn ready_mesh(&self, coord: ChunkCoord) -> Option<(&MeshStamp, &Mesh)> {
        let record = self.records.get(&coord)?;
        match &record.mesh {
            MeshState::CpuReady { stamp, mesh } => Some((stamp, mesh)),
            _ => None,
        }
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
    pub fn is_idle(&self) -> bool {
        self.in_flight.is_none()
            && !self.records.values().any(|record| {
                matches!(
                    record.residency,
                    ResidencyState::LoadQueued | ResidencyState::Loading
                ) || matches!(record.mesh, MeshState::Dirty(_) | MeshState::Meshing(_))
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
            self.records.insert(
                coord,
                ChunkRecord {
                    token,
                    residency: ResidencyState::LoadQueued,
                    mesh_generation: 0,
                    mesh: MeshState::NotRequired,
                },
            );
            self.load_queue.push(LoadQueueEntry {
                priority: self.priority(coord),
                coord,
                token,
            });
            if self.demand.render.contains(&coord) {
                self.invalidate_mesh(coord)?;
            }
        }
        Ok(())
    }

    fn finalize_evictions(&mut self) {
        let evicted: Vec<_> = self
            .records
            .iter()
            .filter_map(|(coord, record)| {
                matches!(record.residency, ResidencyState::EvictPending(_)).then_some(*coord)
            })
            .collect();
        for coord in evicted {
            self.records.remove(&coord);
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
                    priority: self.priority(coord),
                    coord,
                    token: record.token,
                    mesh_generation: stamp.mesh_generation,
                });
            }
        }
    }

    fn invalidate_neighbors(&mut self, coord: ChunkCoord) -> Result<(), RuntimeError> {
        for face in Face::ALL {
            if let Some(neighbor) = coord.neighbor(face) {
                self.invalidate_mesh(neighbor)?;
            }
        }
        Ok(())
    }

    fn invalidate_mesh(&mut self, coord: ChunkCoord) -> Result<(), RuntimeError> {
        let Some(record) = self.records.get_mut(&coord) else {
            return Ok(());
        };
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
            record.mesh = if self.demand.render.contains(&coord) {
                MeshState::WaitingForNeighbors
            } else {
                MeshState::NotRequired
            };
            return Ok(());
        }

        record.mesh = MeshState::WaitingForNeighbors;
        if let Some(stamp) = self.current_mesh_stamp(coord) {
            let record = self
                .records
                .get_mut(&coord)
                .ok_or(RuntimeError::WorkerDisconnected)?;
            record.mesh = MeshState::Dirty(stamp);
            self.mesh_queue.push(MeshQueueEntry {
                priority: self.priority(coord),
                coord,
                token: stamp.request_token,
                mesh_generation: stamp.mesh_generation,
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
            mesh_generation: record.mesh_generation,
            center_content_generation: content_generation,
            neighbors,
        })
    }

    fn neighbor_stamp(&self, coord: ChunkCoord, face: Face) -> Option<NeighborStamp> {
        let neighbor_coord = coord.neighbor(face)?;
        let neighbor = self.records.get(&neighbor_coord)?;
        match neighbor.residency {
            ResidencyState::CpuResident {
                content_generation, ..
            } => Some(NeighborStamp::Resident {
                coord: neighbor_coord,
                token: neighbor.token,
                content_generation,
            }),
            ResidencyState::KnownAbsent => Some(NeighborStamp::KnownAbsent {
                coord: neighbor_coord,
                token: neighbor.token,
            }),
            _ => None,
        }
    }

    fn snapshot(&self, stamp: MeshStamp) -> Option<OwnedMeshingSnapshot> {
        if self.current_mesh_stamp(stamp.coord)? != stamp {
            return None;
        }
        let center_record = self.records.get(&stamp.coord)?;
        let ResidencyState::CpuResident { chunk: center, .. } = &center_record.residency else {
            return None;
        };
        let slab = |face| {
            let neighbor_coord = stamp.coord.neighbor(face)?;
            let neighbor = self.records.get(&neighbor_coord)?;
            match &neighbor.residency {
                ResidencyState::CpuResident { chunk, .. } => {
                    Some(FaceSlab::from_neighbor(face, chunk))
                }
                ResidencyState::KnownAbsent => Some(FaceSlab::known_air()),
                _ => None,
            }
        };
        Some(OwnedMeshingSnapshot::new(
            center.clone(),
            [
                slab(Face::NegativeX)?,
                slab(Face::PositiveX)?,
                slab(Face::NegativeY)?,
                slab(Face::PositiveY)?,
                slab(Face::NegativeZ)?,
                slab(Face::PositiveZ)?,
            ],
        ))
    }

    fn integrate_result(&mut self, result: WorkerResult) -> Result<(), RuntimeError> {
        match result {
            WorkerResult::Load {
                coord,
                token,
                source,
            } => self.integrate_load(coord, token, source),
            WorkerResult::Mesh(result) => self.integrate_mesh(result.stamp, result.mesh),
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
        self.invalidate_mesh(coord)?;
        self.invalidate_neighbors(coord)
    }

    fn integrate_mesh(&mut self, stamp: MeshStamp, mesh: Mesh) -> Result<(), RuntimeError> {
        let accepted = self.current_mesh_stamp(stamp.coord) == Some(stamp)
            && matches!(
                self.records.get(&stamp.coord).map(|record| &record.mesh),
                Some(MeshState::Meshing(active)) if *active == stamp
            );
        if !accepted {
            self.metrics.stale_mesh_results += 1;
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

    fn dispatch_one(&mut self) -> Result<(), RuntimeError> {
        let mesh = self.pop_valid_mesh();
        let load = self.pop_valid_load();
        let force_load = mesh.is_some()
            && load.is_some()
            && self.consecutive_mesh_dispatches >= MESH_BURST_BEFORE_LOAD;

        if force_load {
            if let Some(mesh) = mesh {
                self.mesh_queue.push(mesh);
            }
            self.metrics.fairness_load_dispatches += 1;
            if let Some(load) = load {
                return self.dispatch_load(load);
            }
        }

        if let Some(mesh) = mesh {
            if let Some(load) = load {
                self.load_queue.push(load);
            }
            return self.dispatch_mesh(mesh);
        }
        if let Some(load) = load {
            return self.dispatch_load(load);
        }
        Ok(())
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
        let Some(snapshot) = self.snapshot(stamp) else {
            return Ok(());
        };
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

    fn priority(&self, coord: ChunkCoord) -> Reverse<(u128, ChunkCoord)> {
        let squared = axis_distance(coord.x, self.center.x)
            + axis_distance(coord.y, self.center.y)
            + axis_distance(coord.z, self.center.z);
        Reverse((squared, coord))
    }
}

fn axis_distance(left: i32, right: i32) -> u128 {
    let difference = i128::from(left) - i128::from(right);
    difference.unsigned_abs().pow(2)
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
        runtime.invalidate_neighbors(neighbor)?;
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
        runtime.invalidate_mesh(center)?;
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
            priority: runtime.priority(load.coord),
            coord: load.coord,
            token: load.token,
            mesh_generation: 0,
        });
        // A synthetic valid Dirty state isolates the deterministic selector.
        let stamp = MeshStamp {
            coord: load.coord,
            request_token: load.token,
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

    #[test]
    fn real_worker_reaches_a_stable_bounded_state() -> Result<(), RuntimeError> {
        let mut runtime = runtime_with(StreamingConfig {
            render_radius: 0,
            dependency_halo: 1,
            retention_radius: 1,
            hard_resident_cap: 16,
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
