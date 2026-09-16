//! Client adapter between the headless streaming runtime and GPU presentation.
//!
//! The runtime owns authoritative CPU chunks and never sees `wgpu`; this module
//! converts the camera into a demand center, decides what is drawable, and
//! forwards bounded upload/removal commands to whatever implements
//! [`ChunkPresentation`]. Being drawable is a correctness question answered every
//! frame; releasing GPU memory is a budgeted cleanup that may lag behind.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
};

use glam::Vec3;
use tracing::warn;
use veldwake_streaming::{MeshStamp, RuntimeError, StreamingConfig, StreamingRuntime};
use veldwake_voxel::{ChunkCoord, Mesh, WorldCoordinateRangeError, WorldVoxelCoord};

const MEBIBYTE: usize = 1_024 * 1_024;

/// Why a camera position could not become a demand center.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraAnchorError {
    NonFinite { position: [f32; 3] },
    OutsideWorldRange { position: [f32; 3] },
    OutsideChunkRange(WorldCoordinateRangeError),
}

impl Display for CameraAnchorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite { position } => {
                write!(formatter, "camera position {position:?} is not finite")
            }
            Self::OutsideWorldRange { position } => write!(
                formatter,
                "camera position {position:?} does not fit a signed 64-bit voxel coordinate"
            ),
            Self::OutsideChunkRange(error) => error.fmt(formatter),
        }
    }
}

impl Error for CameraAnchorError {}

/// Maps a finite world-unit position to the voxel cell containing it.
///
/// Uses `floor`, never truncation: `-0.001` lies in voxel `-1`, and the result
/// therefore agrees with the Euclidean chunk split used by the voxel crate.
pub fn camera_world_voxel(position: Vec3) -> Result<WorldVoxelCoord, CameraAnchorError> {
    Ok(WorldVoxelCoord::new(
        floor_axis(position.x, position)?,
        floor_axis(position.y, position)?,
        floor_axis(position.z, position)?,
    ))
}

/// Maps a finite world-unit position to the chunk that should center demand.
pub fn camera_chunk(position: Vec3) -> Result<ChunkCoord, CameraAnchorError> {
    let (chunk, _) = camera_world_voxel(position)?
        .split()
        .map_err(CameraAnchorError::OutsideChunkRange)?;
    Ok(chunk)
}

fn floor_axis(value: f32, position: Vec3) -> Result<i64, CameraAnchorError> {
    // 2^63 is exactly representable; `i64` covers [-2^63, 2^63).
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;

    if !value.is_finite() {
        return Err(CameraAnchorError::NonFinite {
            position: position.to_array(),
        });
    }
    let floored = f64::from(value).floor();
    if floored < -LIMIT || floored >= LIMIT {
        return Err(CameraAnchorError::OutsideWorldRange {
            position: position.to_array(),
        });
    }
    // `floored` is integral and inside the range checked above, so the cast is exact.
    Ok(floored as i64)
}

/// A mesh the presentation layer refused to upload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkUploadError {
    TooManyIndices { coord: ChunkCoord, indices: usize },
}

impl Display for ChunkUploadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyIndices { coord, indices } => write!(
                formatter,
                "chunk ({}, {}, {}) mesh has {indices} indices; exceeds u32 draw range",
                coord.x, coord.y, coord.z
            ),
        }
    }
}

impl Error for ChunkUploadError {}

/// Presentation-side chunk residency, as observed by the bridge.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuResidency {
    /// Chunks holding GPU buffers, drawable or not.
    pub resident: usize,
    /// Chunks in the current draw set.
    pub active: usize,
    /// Exact bytes of vertex, index, and model data held on the GPU.
    pub bytes: usize,
}

/// Minimal GPU-side operations the streaming bridge needs.
///
/// The renderer implements this against `wgpu`; tests implement it in memory.
/// Implementations never learn streaming stamps or generations.
pub trait ChunkPresentation {
    /// Exact GPU bytes `mesh` would occupy, computed before any upload.
    fn gpu_payload_bytes(&self, mesh: &Mesh) -> usize;

    /// Uploads or replaces `coord` and makes it drawable. An empty mesh releases
    /// any previous buffers and returns `Ok(0)`.
    fn upsert_chunk(&mut self, coord: ChunkCoord, mesh: &Mesh) -> Result<usize, ChunkUploadError>;

    /// Removes `coord` from the draw set immediately while keeping its buffers.
    fn deactivate_chunk(&mut self, coord: ChunkCoord) -> bool;

    /// Releases `coord`'s buffers. Returns whether anything was held.
    fn remove_chunk(&mut self, coord: ChunkCoord) -> bool;

    fn residency(&self) -> GpuResidency;
}

/// Per-frame presentation limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadBudget {
    pub max_uploads_per_frame: usize,
    pub soft_bytes_per_frame: usize,
    pub max_removals_per_frame: usize,
}

impl Default for UploadBudget {
    fn default() -> Self {
        Self {
            max_uploads_per_frame: 2,
            soft_bytes_per_frame: 4 * MEBIBYTE,
            max_removals_per_frame: 8,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadAdmission {
    Admit,
    /// Larger than the whole soft byte budget; allowed only as the sole upload.
    AdmitOversized,
    Defer,
}

impl UploadBudget {
    #[must_use]
    pub fn admit(&self, uploads_done: usize, bytes_done: usize, bytes: usize) -> UploadAdmission {
        if uploads_done >= self.max_uploads_per_frame {
            return UploadAdmission::Defer;
        }
        if bytes > self.soft_bytes_per_frame {
            return if uploads_done == 0 {
                UploadAdmission::AdmitOversized
            } else {
                UploadAdmission::Defer
            };
        }
        if bytes_done.saturating_add(bytes) > self.soft_bytes_per_frame {
            return UploadAdmission::Defer;
        }
        UploadAdmission::Admit
    }
}

/// Cumulative bridge counters since startup.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BridgeTotals {
    pub demand_updates: u64,
    pub anchor_rejections: u64,
    pub uploads: u64,
    pub upload_bytes: u64,
    pub empty_meshes_presented: u64,
    pub oversized_uploads: u64,
    pub upload_budget_deferrals: u64,
    pub upload_failures: u64,
    pub deactivations: u64,
    pub removals: u64,
    pub removal_budget_hits: u64,
}

/// What one `update` did.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameStreamingReport {
    pub demand_changed: bool,
    pub uploads: usize,
    pub upload_bytes: usize,
    pub deactivations: usize,
    pub removals: usize,
    pub deferred_uploads: usize,
}

/// Owns the streaming runtime and the map of what is currently presented.
pub struct StreamingBridge {
    runtime: StreamingRuntime,
    desired_center: ChunkCoord,
    /// Chunks whose GPU mesh matches this exact stamp and is drawable.
    presented: BTreeMap<ChunkCoord, MeshStamp>,
    /// Deactivated chunks still holding buffers, released under budget.
    pending_removal: BTreeSet<ChunkCoord>,
    budget: UploadBudget,
    totals: BridgeTotals,
    anchor_rejected: bool,
}

impl StreamingBridge {
    pub fn new(
        config: StreamingConfig,
        budget: UploadBudget,
        camera_position: Vec3,
    ) -> Result<Self, BridgeInitError> {
        let center = camera_chunk(camera_position).map_err(BridgeInitError::Anchor)?;
        let runtime = StreamingRuntime::new(config, center).map_err(BridgeInitError::Runtime)?;
        Ok(Self {
            runtime,
            desired_center: center,
            presented: BTreeMap::new(),
            pending_removal: BTreeSet::new(),
            budget,
            totals: BridgeTotals::default(),
            anchor_rejected: false,
        })
    }

    /// Records the chunk the camera occupies. A rejected position keeps the
    /// previous center and is counted; it never saturates into a fake chunk.
    pub fn track_camera(&mut self, position: Vec3) -> Result<bool, CameraAnchorError> {
        match camera_chunk(position) {
            Ok(chunk) => {
                if self.anchor_rejected {
                    self.anchor_rejected = false;
                    warn!(?chunk, "camera anchor is valid again");
                }
                let changed = chunk != self.desired_center;
                self.desired_center = chunk;
                Ok(changed)
            }
            Err(error) => {
                self.totals.anchor_rejections += 1;
                if !self.anchor_rejected {
                    self.anchor_rejected = true;
                    warn!(%error, "camera anchor rejected; keeping previous demand center");
                }
                Err(error)
            }
        }
    }

    /// One bounded frame step: demand, runtime poll, draw-set reconciliation,
    /// budgeted uploads, budgeted releases.
    pub fn update<P: ChunkPresentation>(
        &mut self,
        presentation: &mut P,
    ) -> Result<FrameStreamingReport, RuntimeError> {
        let mut report = FrameStreamingReport::default();

        if self.desired_center != self.runtime.center() {
            self.runtime.set_demand_center(self.desired_center)?;
            self.totals.demand_updates += 1;
            report.demand_changed = true;
        }
        self.runtime.poll()?;

        report.deactivations = self.reconcile_draw_set(presentation);
        let (uploads, upload_bytes, deferred) = self.upload_ready_meshes(presentation);
        report.uploads = uploads;
        report.upload_bytes = upload_bytes;
        report.deferred_uploads = deferred;
        report.removals = self.release_pending(presentation);
        Ok(report)
    }

    /// Any presented mesh whose stamp is no longer the current render-demand
    /// ready mesh stops drawing now, regardless of release budget.
    fn reconcile_draw_set<P: ChunkPresentation>(&mut self, presentation: &mut P) -> usize {
        let render = &self.runtime.demand().render;
        let stale: Vec<ChunkCoord> = self
            .presented
            .iter()
            .filter(|(coord, stamp)| {
                !render.contains(*coord)
                    || self
                        .runtime
                        .ready_mesh(**coord)
                        .is_none_or(|(current, _)| current != *stamp)
            })
            .map(|(coord, _)| *coord)
            .collect();
        for coord in &stale {
            presentation.deactivate_chunk(*coord);
            self.presented.remove(coord);
            self.pending_removal.insert(*coord);
        }
        self.totals.deactivations += stale.len() as u64;
        stale.len()
    }

    fn upload_ready_meshes<P: ChunkPresentation>(
        &mut self,
        presentation: &mut P,
    ) -> (usize, usize, usize) {
        let center = self.runtime.center();
        let mut candidates: Vec<(u128, ChunkCoord, MeshStamp, &Mesh)> = self
            .runtime
            .render_ready_meshes()
            .filter(|(coord, stamp, _)| self.presented.get(coord) != Some(*stamp))
            .map(|(coord, stamp, mesh)| (squared_distance(coord, center), coord, *stamp, mesh))
            .collect();
        candidates.sort_by_key(|(distance, coord, _, _)| (*distance, *coord));

        let mut uploads = 0;
        let mut bytes_done = 0;
        let mut deferred = 0;
        let mut presented_now = Vec::new();
        for (_, coord, stamp, mesh) in candidates {
            if mesh.indices().is_empty() {
                // Nothing reaches the GPU; presenting it only clears stale buffers.
                match presentation.upsert_chunk(coord, mesh) {
                    Ok(_) => {
                        self.totals.empty_meshes_presented += 1;
                        presented_now.push((coord, stamp));
                    }
                    Err(error) => {
                        self.totals.upload_failures += 1;
                        warn!(%error, "empty chunk mesh rejected by presentation");
                    }
                }
                continue;
            }

            let bytes = presentation.gpu_payload_bytes(mesh);
            match self.budget.admit(uploads, bytes_done, bytes) {
                UploadAdmission::Defer => {
                    deferred += 1;
                    continue;
                }
                UploadAdmission::AdmitOversized => self.totals.oversized_uploads += 1,
                UploadAdmission::Admit => {}
            }
            match presentation.upsert_chunk(coord, mesh) {
                Ok(uploaded) => {
                    uploads += 1;
                    bytes_done += uploaded;
                    presented_now.push((coord, stamp));
                }
                Err(error) => {
                    self.totals.upload_failures += 1;
                    warn!(%error, "chunk mesh upload failed; chunk stays undrawn");
                }
            }
        }
        for (coord, stamp) in presented_now {
            self.presented.insert(coord, stamp);
            self.pending_removal.remove(&coord);
        }
        self.totals.uploads += uploads as u64;
        self.totals.upload_bytes += bytes_done as u64;
        self.totals.upload_budget_deferrals += deferred as u64;
        (uploads, bytes_done, deferred)
    }

    fn release_pending<P: ChunkPresentation>(&mut self, presentation: &mut P) -> usize {
        let budget = self.budget.max_removals_per_frame;
        let batch: Vec<ChunkCoord> = self.pending_removal.iter().copied().take(budget).collect();
        for coord in &batch {
            presentation.remove_chunk(*coord);
            self.pending_removal.remove(coord);
        }
        if batch.len() == budget && !self.pending_removal.is_empty() {
            self.totals.removal_budget_hits += 1;
        }
        self.totals.removals += batch.len() as u64;
        batch.len()
    }

    #[must_use]
    pub const fn runtime(&self) -> &StreamingRuntime {
        &self.runtime
    }

    #[must_use]
    pub const fn totals(&self) -> &BridgeTotals {
        &self.totals
    }

    #[must_use]
    pub const fn desired_center(&self) -> ChunkCoord {
        self.desired_center
    }

    #[must_use]
    pub fn presented_count(&self) -> usize {
        self.presented.len()
    }

    #[must_use]
    pub fn pending_removal_count(&self) -> usize {
        self.pending_removal.len()
    }

    #[cfg(test)]
    pub fn presented_stamp(&self, coord: ChunkCoord) -> Option<&MeshStamp> {
        self.presented.get(&coord)
    }

    #[cfg(test)]
    pub const fn runtime_mut(&mut self) -> &mut StreamingRuntime {
        &mut self.runtime
    }
}

#[derive(Debug)]
pub enum BridgeInitError {
    Anchor(CameraAnchorError),
    Runtime(RuntimeError),
}

impl Display for BridgeInitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Anchor(error) => write!(formatter, "initial camera anchor rejected: {error}"),
            Self::Runtime(error) => write!(formatter, "streaming runtime failed: {error}"),
        }
    }
}

impl Error for BridgeInitError {}

fn squared_distance(coord: ChunkCoord, center: ChunkCoord) -> u128 {
    let axis = |left: i32, right: i32| (i128::from(left) - i128::from(right)).unsigned_abs().pow(2);
    axis(coord.x, center.x) + axis(coord.y, center.y) + axis(coord.z, center.z)
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, thread, time::Duration};

    use glam::Vec3;
    use veldwake_streaming::StreamingConfig;
    use veldwake_voxel::{CHUNK_EDGE, ChunkCoord, Mesh, WorldVoxelCoord};

    use super::{
        CameraAnchorError, ChunkPresentation, ChunkUploadError, GpuResidency, StreamingBridge,
        UploadAdmission, UploadBudget, camera_chunk, camera_world_voxel,
    };

    #[derive(Default)]
    struct FakePresentation {
        chunks: BTreeMap<ChunkCoord, (usize, bool)>,
        upserts: usize,
        removals: usize,
    }

    impl ChunkPresentation for FakePresentation {
        fn gpu_payload_bytes(&self, mesh: &Mesh) -> usize {
            mesh.vertices().len() * 24 + mesh.indices().len() * 4 + 16
        }

        fn upsert_chunk(
            &mut self,
            coord: ChunkCoord,
            mesh: &Mesh,
        ) -> Result<usize, ChunkUploadError> {
            self.upserts += 1;
            if mesh.indices().is_empty() {
                self.chunks.remove(&coord);
                return Ok(0);
            }
            let bytes = self.gpu_payload_bytes(mesh);
            self.chunks.insert(coord, (bytes, true));
            Ok(bytes)
        }

        fn deactivate_chunk(&mut self, coord: ChunkCoord) -> bool {
            self.chunks
                .get_mut(&coord)
                .map(|(_, active)| *active = false)
                .is_some()
        }

        fn remove_chunk(&mut self, coord: ChunkCoord) -> bool {
            self.removals += 1;
            self.chunks.remove(&coord).is_some()
        }

        fn residency(&self) -> GpuResidency {
            GpuResidency {
                resident: self.chunks.len(),
                active: self.chunks.values().filter(|(_, active)| *active).count(),
                bytes: self.chunks.values().map(|(bytes, _)| *bytes).sum(),
            }
        }
    }

    impl FakePresentation {
        fn active(&self) -> Vec<ChunkCoord> {
            self.chunks
                .iter()
                .filter(|(_, (_, active))| *active)
                .map(|(coord, _)| *coord)
                .collect()
        }
    }

    fn settle(bridge: &mut StreamingBridge, fake: &mut FakePresentation) {
        for _ in 0..20_000 {
            let report = match bridge.update(fake) {
                Ok(report) => report,
                Err(error) => panic!("streaming update failed: {error}"),
            };
            let quiet = report.uploads == 0
                && report.deactivations == 0
                && report.removals == 0
                && report.deferred_uploads == 0;
            if quiet && bridge.runtime().is_idle() && bridge.pending_removal_count() == 0 {
                return;
            }
            thread::sleep(Duration::from_micros(50));
        }
        panic!("bridge did not settle");
    }

    fn bridge_at(position: Vec3, budget: UploadBudget) -> StreamingBridge {
        match StreamingBridge::new(StreamingConfig::default(), budget, position) {
            Ok(bridge) => bridge,
            Err(error) => panic!("bridge failed to start: {error}"),
        }
    }

    #[test]
    fn camera_voxel_uses_floor_on_every_axis_including_negatives() {
        let edge = CHUNK_EDGE as f32;
        let cases: [(f32, i64, i32, usize); 7] = [
            (0.0, 0, 0, 0),
            (31.999, 31, 0, 31),
            (edge, 32, 1, 0),
            (-0.001, -1, -1, 31),
            (-1.0, -1, -1, 31),
            (-edge, -32, -1, 0),
            (-32.001, -33, -2, 31),
        ];
        for (value, voxel, chunk, local) in cases {
            let position = Vec3::new(value, value, value);
            let world = match camera_world_voxel(position) {
                Ok(world) => world,
                Err(error) => panic!("{value} rejected: {error}"),
            };
            assert_eq!(
                world,
                WorldVoxelCoord::new(voxel, voxel, voxel),
                "value {value}"
            );
            let (chunk_coord, local_coord) = match world.split() {
                Ok(split) => split,
                Err(error) => panic!("{value} split failed: {error}"),
            };
            assert_eq!(
                chunk_coord,
                ChunkCoord::new(chunk, chunk, chunk),
                "value {value}"
            );
            assert_eq!(local_coord.x(), local, "value {value}");
        }
        // Truncation toward zero would put -0.001 in chunk 0; floor must not.
        assert_ne!((-0.001_f32) as i64, -1);
    }

    #[test]
    fn non_finite_and_out_of_range_positions_are_rejected_explicitly() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let position = Vec3::new(1.0, bad, 1.0);
            // `NaN != NaN`, so compare structurally rather than by equality.
            assert!(
                matches!(
                    camera_chunk(position),
                    Err(CameraAnchorError::NonFinite { position: reported })
                        if reported[0] == 1.0 && reported[1].to_bits() == bad.to_bits()
                ),
                "{bad} must be rejected as non-finite"
            );
        }
        let huge = Vec3::new(f32::MAX, 0.0, 0.0);
        assert_eq!(
            camera_chunk(huge),
            Err(CameraAnchorError::OutsideWorldRange {
                position: huge.to_array()
            })
        );
        // Representable as a voxel but not as an i32 chunk quotient.
        let beyond_chunks = Vec3::new(1.0e12, 0.0, 0.0);
        assert!(matches!(
            camera_chunk(beyond_chunks),
            Err(CameraAnchorError::OutsideChunkRange(_))
        ));
    }

    #[test]
    fn upload_budget_admits_two_small_uploads_and_one_oversized_only_alone() {
        let budget = UploadBudget {
            max_uploads_per_frame: 2,
            soft_bytes_per_frame: 1_000,
            max_removals_per_frame: 8,
        };
        assert_eq!(budget.admit(0, 0, 400), UploadAdmission::Admit);
        assert_eq!(budget.admit(1, 400, 400), UploadAdmission::Admit);
        assert_eq!(budget.admit(1, 400, 601), UploadAdmission::Defer);
        assert_eq!(budget.admit(2, 800, 1), UploadAdmission::Defer);
        assert_eq!(budget.admit(0, 0, 5_000), UploadAdmission::AdmitOversized);
        assert_eq!(budget.admit(1, 400, 5_000), UploadAdmission::Defer);
    }

    #[test]
    fn camera_tracking_only_changes_center_across_chunk_boundaries() {
        let mut bridge = bridge_at(Vec3::new(5.0, 5.0, 5.0), UploadBudget::default());
        assert_eq!(bridge.desired_center(), ChunkCoord::new(0, 0, 0));
        assert_eq!(bridge.track_camera(Vec3::new(31.9, 5.0, 5.0)), Ok(false));
        assert_eq!(bridge.track_camera(Vec3::new(32.0, 5.0, 5.0)), Ok(true));
        assert_eq!(bridge.desired_center(), ChunkCoord::new(1, 0, 0));
        assert_eq!(bridge.track_camera(Vec3::new(-0.001, 5.0, 5.0)), Ok(true));
        assert_eq!(bridge.desired_center(), ChunkCoord::new(-1, 0, 0));

        let rejected = bridge.track_camera(Vec3::new(f32::NAN, 5.0, 5.0));
        assert!(matches!(rejected, Err(CameraAnchorError::NonFinite { .. })));
        assert_eq!(bridge.desired_center(), ChunkCoord::new(-1, 0, 0));
        assert_eq!(bridge.totals().anchor_rejections, 1);
    }

    #[test]
    fn settled_bridge_presents_exactly_the_render_demand_ready_meshes() {
        let mut fake = FakePresentation::default();
        let mut bridge = bridge_at(Vec3::new(5.0, 5.0, 5.0), UploadBudget::default());
        settle(&mut bridge, &mut fake);

        let render = &bridge.runtime().demand().render;
        assert_eq!(bridge.presented_count(), render.len());
        let ready: Vec<_> = bridge.runtime().render_ready_meshes().collect();
        assert_eq!(ready.len(), render.len());
        for (coord, stamp, mesh) in ready {
            assert!(render.contains(&coord));
            assert_eq!(bridge.presented_stamp(coord), Some(stamp));
            let held = fake.chunks.get(&coord);
            if mesh.indices().is_empty() {
                assert!(held.is_none(), "empty mesh must hold no GPU bytes");
            } else {
                assert!(
                    matches!(held, Some((_, true))),
                    "non-empty mesh must be active"
                );
            }
        }
        // Dependency-only and retention-only chunks are never drawn.
        for coord in fake.active() {
            assert!(render.contains(&coord));
        }
        assert!(bridge.totals().uploads > 0);
        assert!(bridge.totals().empty_meshes_presented > 0);

        // Same stamps again: nothing is re-uploaded.
        let uploads_before = fake.upserts;
        for _ in 0..5 {
            let report = match bridge.update(&mut fake) {
                Ok(report) => report,
                Err(error) => panic!("update failed: {error}"),
            };
            assert_eq!(report.uploads, 0);
        }
        assert_eq!(fake.upserts, uploads_before);
    }

    #[test]
    fn leaving_render_demand_deactivates_immediately_and_releases_under_budget() {
        let budget = UploadBudget {
            max_removals_per_frame: 2,
            ..UploadBudget::default()
        };
        let mut fake = FakePresentation::default();
        let mut bridge = bridge_at(Vec3::new(5.0, 5.0, 5.0), budget);
        settle(&mut bridge, &mut fake);
        let held_before = fake.residency().resident;
        assert!(
            held_before > 2,
            "need a backlog larger than one release batch"
        );

        assert_eq!(bridge.track_camera(Vec3::new(400.0, 5.0, 400.0)), Ok(true));
        let report = match bridge.update(&mut fake) {
            Ok(report) => report,
            Err(error) => panic!("update failed: {error}"),
        };
        assert!(report.demand_changed);
        assert_eq!(
            fake.residency().active,
            0,
            "stale meshes must stop drawing now"
        );
        assert_eq!(bridge.presented_count(), 0);
        assert_eq!(report.removals, 2);
        assert!(
            fake.residency().resident > 0,
            "release is budgeted, not immediate"
        );
        assert_eq!(bridge.totals().removal_budget_hits, 1);

        settle(&mut bridge, &mut fake);
        assert_eq!(fake.residency().resident, 0);
        assert_eq!(bridge.pending_removal_count(), 0);
    }

    #[test]
    fn reentry_rebuilds_with_new_stamps() {
        let mut fake = FakePresentation::default();
        let mut bridge = bridge_at(Vec3::new(5.0, 5.0, 5.0), UploadBudget::default());
        settle(&mut bridge, &mut fake);
        let origin = ChunkCoord::new(0, 0, 0);
        let first = *bridge
            .presented_stamp(origin)
            .unwrap_or_else(|| panic!("origin not presented"));

        assert_eq!(bridge.track_camera(Vec3::new(400.0, 5.0, 400.0)), Ok(true));
        settle(&mut bridge, &mut fake);
        assert_eq!(bridge.presented_stamp(origin), None);

        assert_eq!(bridge.track_camera(Vec3::new(5.0, 5.0, 5.0)), Ok(true));
        settle(&mut bridge, &mut fake);
        let second = *bridge
            .presented_stamp(origin)
            .unwrap_or_else(|| panic!("origin not re-presented"));
        assert_ne!(first.request_token, second.request_token);
        assert!(matches!(fake.chunks.get(&origin), Some((_, true))));
    }

    #[test]
    fn oversized_meshes_upload_alone_and_are_counted() {
        let budget = UploadBudget {
            max_uploads_per_frame: 2,
            soft_bytes_per_frame: 64,
            max_removals_per_frame: 8,
        };
        let mut fake = FakePresentation::default();
        let mut bridge = bridge_at(Vec3::new(5.0, 5.0, 5.0), budget);
        // Let every mesh become ready before the bridge uploads anything, so a
        // single update sees the whole backlog compete for one frame's budget.
        for _ in 0..20_000 {
            if let Err(error) = bridge.runtime_mut().poll() {
                panic!("runtime poll failed: {error}");
            }
            if bridge.runtime().is_idle() {
                break;
            }
            thread::sleep(Duration::from_micros(50));
        }
        assert!(bridge.runtime().is_idle());
        let non_empty = bridge
            .runtime()
            .render_ready_meshes()
            .filter(|(_, _, mesh)| !mesh.indices().is_empty())
            .count();
        assert!(non_empty > 1, "need several oversized candidates");

        let report = match bridge.update(&mut fake) {
            Ok(report) => report,
            Err(error) => panic!("update failed: {error}"),
        };
        assert_eq!(
            report.uploads, 1,
            "an oversized mesh is the frame's only upload"
        );
        assert_eq!(report.deferred_uploads, non_empty - 1);
        assert_eq!(bridge.totals().oversized_uploads, 1);
        assert_eq!(
            bridge.totals().upload_budget_deferrals,
            (non_empty - 1) as u64
        );

        let mut max_uploads_in_one_frame = report.uploads;
        for _ in 0..non_empty + 2 {
            let report = match bridge.update(&mut fake) {
                Ok(report) => report,
                Err(error) => panic!("update failed: {error}"),
            };
            max_uploads_in_one_frame = max_uploads_in_one_frame.max(report.uploads);
        }
        assert_eq!(max_uploads_in_one_frame, 1);
        assert_eq!(bridge.totals().oversized_uploads, bridge.totals().uploads);
        assert_eq!(
            bridge.presented_count(),
            bridge.runtime().demand().render.len()
        );
    }
}
