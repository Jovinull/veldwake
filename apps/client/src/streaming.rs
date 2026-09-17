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
use tracing::{error, warn};
use veldwake_streaming::{
    InvalidationCause, LodLevel, MeshStamp, RuntimeError, StreamingConfig, StreamingRuntime,
};
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

/// Why an atomic group commit was refused by the presentation.
///
/// Carries the first member that had no staged replacement. Nothing was
/// changed: a group is the unit of seam coherence, so a partial swap could
/// draw a mixed seam and is never performed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationCommitError {
    pub coord: ChunkCoord,
    pub group_size: usize,
}

impl Display for PresentationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "chunk ({}, {}, {}) of a {}-chunk transition group has no staged replacement",
            self.coord.x, self.coord.y, self.coord.z, self.group_size
        )
    }
}

impl Error for PresentationCommitError {}

/// Bytes of chunk-mesh buffers owned by the presentation at one instant.
///
/// Committed and staged are reported separately and the peak is taken on
/// their simultaneous sum; adding two independently observed peaks would
/// overstate the real high-water mark. This is not global GPU memory: depth
/// textures, pipelines, debug resources, and driver/wgpu retention are outside
/// this accounting boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChunkMeshGpuBytes {
    /// Bytes of committed meshes: drawable, or deactivated and awaiting a
    /// budgeted release.
    pub committed: usize,
    /// Bytes of replacements uploaded but not drawn yet.
    pub staged: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct GpuPeakObservations {
    committed: usize,
    staged: usize,
    total: usize,
}

impl GpuPeakObservations {
    fn observe<P: ChunkPresentation>(&mut self, presentation: &P) -> ChunkMeshGpuBytes {
        let residency = presentation.residency();
        let sample = ChunkMeshGpuBytes {
            committed: residency.bytes(),
            staged: residency.staged_bytes(),
        };
        self.committed = self.committed.max(sample.committed);
        self.staged = self.staged.max(sample.staged);
        self.total = self.total.max(sample.total());
        sample
    }
}

impl ChunkMeshGpuBytes {
    #[must_use]
    pub const fn total(&self) -> usize {
        self.committed + self.staged
    }
}

/// GPU residency of one presentation level.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LevelResidency {
    /// Chunks holding GPU buffers, drawable or not.
    pub resident: usize,
    /// Chunks in the current draw set.
    pub active: usize,
    /// Exact bytes of vertex, index, and model data held on the GPU by
    /// active (committed) meshes.
    pub bytes: usize,
    /// Quads held by active meshes.
    pub quads: usize,
    /// Replacement meshes staged but not yet drawable, and their bytes.
    pub staged: usize,
    pub staged_bytes: usize,
}

/// Presentation-side chunk residency per level, as observed by the bridge.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuResidency {
    pub lod0: LevelResidency,
    pub lod1: LevelResidency,
}

impl GpuResidency {
    #[must_use]
    pub const fn resident(&self) -> usize {
        self.lod0.resident + self.lod1.resident
    }

    #[must_use]
    pub const fn active(&self) -> usize {
        self.lod0.active + self.lod1.active
    }

    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.lod0.bytes + self.lod1.bytes
    }

    #[must_use]
    pub const fn quads(&self) -> usize {
        self.lod0.quads + self.lod1.quads
    }

    #[must_use]
    pub const fn staged(&self) -> usize {
        self.lod0.staged + self.lod1.staged
    }

    #[must_use]
    pub const fn staged_bytes(&self) -> usize {
        self.lod0.staged_bytes + self.lod1.staged_bytes
    }
}

/// Minimal GPU-side operations the streaming bridge needs.
///
/// The renderer implements this against `wgpu`; tests implement it in memory.
/// Implementations never learn streaming stamps or generations.
pub trait ChunkPresentation {
    /// Exact GPU bytes `mesh` would occupy, computed before any upload.
    fn gpu_payload_bytes(&self, mesh: &Mesh) -> usize;

    /// Uploads a replacement for `coord` at presentation level `lod` without
    /// drawing it; the current mesh, if any, keeps drawing. The level is the
    /// only streaming fact the presentation learns; it never sees tokens,
    /// generations, or the full stamp. Staging an empty mesh holds no buffers
    /// and returns `Ok(0)`; committing it later removes the old mesh.
    ///
    /// Ordering contract, because the caller cannot observe the inside of this
    /// call: reject before touching any state, then release the coordinate's
    /// previous replacement, then acquire the new one. An implementation must
    /// never hold two replacements of one chunk at the same instant, and must
    /// never release valid staging on a rejected call. The bridge restages a
    /// coordinate only when its staged stamp is no longer that chunk's target,
    /// and such a replacement can never be committed, so releasing it first is
    /// always safe. The committed mesh is never touched here.
    fn stage_chunk(
        &mut self,
        coord: ChunkCoord,
        lod: LodLevel,
        mesh: &Mesh,
    ) -> Result<usize, ChunkUploadError>;

    /// Atomically makes the staged replacements of every member of `group`
    /// the drawn meshes and releases the previous ones.
    ///
    /// All-or-nothing. The implementation must verify that every member has a
    /// staged replacement *before* changing any slot and return
    /// [`PresentationCommitError`] without touching anything when one is
    /// missing. The group is the unit of seam coherence: committing part of
    /// it could draw a pair that mixes an old and a new seam.
    fn commit_staged_group(&mut self, group: &[ChunkCoord]) -> Result<(), PresentationCommitError>;

    /// Drops a staged replacement that will never be committed.
    fn discard_staged(&mut self, coord: ChunkCoord) -> bool;

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

/// Outcome of anchoring one camera position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraAnchor {
    Unchanged,
    Moved(ChunkCoord),
    /// Already logged and counted; the previous center stays in force.
    Rejected(CameraAnchorError),
}

/// Cumulative bridge counters since startup.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BridgeTotals {
    pub demand_updates: u64,
    pub anchor_rejections: u64,
    pub uploads: u64,
    pub upload_bytes: u64,
    pub uploads_lod0: u64,
    pub uploads_lod1: u64,
    pub upload_bytes_lod0: u64,
    pub upload_bytes_lod1: u64,
    pub empty_meshes_presented: u64,
    pub oversized_uploads: u64,
    pub upload_budget_deferrals: u64,
    pub upload_failures: u64,
    pub deactivations: u64,
    pub removals: u64,
    pub removal_budget_hits: u64,
    /// Atomic transition commits and the chunks they switched.
    pub transition_commits: u64,
    pub transition_chunks: u64,
    /// Staged replacements re-staged because their target moved on.
    pub restaged: u64,
    /// Staged replacements discarded because their chunk no longer needed one.
    pub staged_discarded: u64,
    /// Atomic group commits the presentation refused because a member had no
    /// staged replacement. Nothing was committed on either side.
    pub presentation_commit_failures: u64,
    /// Groups whose runtime-held preflight and presentation swap succeeded but
    /// whose CPU commit count was short. Exclusive borrowing prevents an
    /// intervening runtime mutation, so this must stay zero; a non-zero value
    /// is an internal invariant failure, never a budget effect.
    pub commit_invariant_failures: u64,
    /// Highest bytes held by staged (undrawn) replacements at one time, as
    /// the presentation reports them.
    pub peak_staged_bytes: usize,
    /// Highest bytes held by committed meshes at one time.
    pub peak_chunk_mesh_committed_bytes: usize,
    /// Highest simultaneous `committed + staged` chunk-mesh bytes observed at
    /// any presentation mutation boundary. This excludes all non-chunk GPU
    /// resources and is never the sum of peaks from different instants.
    pub peak_chunk_mesh_total_bytes: usize,
}

/// Presentation gaps: render-demand chunks that were drawn, stopped being
/// drawn while still in render demand, and had not returned yet.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GapStats {
    /// Gaps opened by a level change of the chunk itself.
    pub lod_gaps: u64,
    /// Gaps opened by a neighbor's presentation change.
    pub neighbor_presentation_gaps: u64,
    /// Gaps opened by render-membership churn.
    pub membership_gaps: u64,
    /// Gaps opened by data: loads, evictions, content changes.
    pub data_gaps: u64,
    /// Gaps with no recorded cause.
    pub unattributed_gaps: u64,
    /// Sum over closed gaps of their length in update frames.
    pub gap_frames_total: u64,
    /// Longest closed gap in update frames.
    pub max_gap_frames: u64,
    /// Highest number of simultaneously missing chunks observed in one update.
    pub max_simultaneous_missing: usize,
    /// Chunks missing right now.
    pub current_missing: usize,
    /// Update frames in which at least one chunk was missing.
    pub frames_with_missing: u64,
    /// Non-empty render-demand meshes that are CPU-ready but not drawn this
    /// update. These are real known-geometry coverage deficits, even when the
    /// cause is only the bounded upload pipeline rather than seam starvation.
    pub ready_undrawn_now: usize,
    /// Highest `ready_undrawn_now` observed in one update.
    pub ready_undrawn_max: usize,
    /// Update frames with at least one ready-but-undrawn chunk.
    pub ready_undrawn_frames: u64,
    /// Sum over updates of `ready_undrawn_now`.
    pub ready_undrawn_chunk_frames: u64,
    /// Render-demand records not yet known absent, committed, or CPU-ready.
    /// Geometry is not known yet, so this is frontier pipeline latency rather
    /// than a proven ready-geometry hole.
    pub frontier_pipeline_pending_now: usize,
    pub frontier_pipeline_pending_max: usize,
    /// Non-empty CPU-ready meshes that have not acquired matching staging;
    /// normally these are waiting behind the per-frame upload budget.
    pub ready_awaiting_upload_now: usize,
    pub ready_awaiting_upload_max: usize,
    /// Non-empty CPU-ready meshes already staged but withheld because another
    /// member of their seam-coherent transition group is not ready/staged (or
    /// because the presentation refused the atomic swap).
    pub ready_blocked_transition_now: usize,
    pub ready_blocked_transition_max: usize,
    /// Non-empty meshes the runtime says are committed but the bridge does not
    /// have under that exact stamp. This is an invariant failure, never budget
    /// latency, and must remain zero.
    pub committed_missing_now: usize,
    pub committed_missing_max: usize,
    /// Transition groups that could not commit this update, and the largest.
    pub blocked_groups_now: usize,
    pub blocked_group_max: usize,
    /// Undrawn members of blocked groups that also hold a drawn member: the
    /// entrants legitimately waiting for a drawn neighbor whose seam toward
    /// them changes. Anything else undrawn waits only for its own pipeline.
    pub constrained_undrawn_now: usize,
    pub constrained_undrawn_max: usize,
}

impl GapStats {
    #[must_use]
    pub const fn closed_gaps(&self) -> u64 {
        self.lod_gaps
            + self.neighbor_presentation_gaps
            + self.membership_gaps
            + self.data_gaps
            + self.unattributed_gaps
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadyUndrawnReason {
    AwaitingUpload,
    BlockedByTransition,
}

fn classify_ready_undrawn(
    is_presented: bool,
    is_empty: bool,
    has_matching_staging: bool,
) -> Option<ReadyUndrawnReason> {
    if is_presented || is_empty {
        None
    } else if has_matching_staging {
        Some(ReadyUndrawnReason::BlockedByTransition)
    } else {
        Some(ReadyUndrawnReason::AwaitingUpload)
    }
}

/// What one `update` did.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameStreamingReport {
    pub demand_changed: bool,
    /// Meshes staged this frame (uploads under budget).
    pub uploads: usize,
    pub upload_bytes: usize,
    pub deactivations: usize,
    pub removals: usize,
    pub deferred_uploads: usize,
    /// Transition groups committed this frame and chunks they switched.
    pub commits: usize,
    pub committed_chunks: usize,
}

/// Owns the streaming runtime and the map of what is currently presented.
pub struct StreamingBridge {
    runtime: StreamingRuntime,
    desired_center: ChunkCoord,
    /// Chunks whose GPU mesh matches this exact stamp and is drawable.
    presented: BTreeMap<ChunkCoord, MeshStamp>,
    /// Replacements uploaded but not drawn: the stamp they were built for and
    /// their GPU bytes.
    staged: BTreeMap<ChunkCoord, (MeshStamp, usize)>,
    /// Deactivated chunks still holding buffers, released under budget.
    pending_removal: BTreeSet<ChunkCoord>,
    budget: UploadBudget,
    totals: BridgeTotals,
    anchor_rejected: bool,
    /// Update counter for gap attribution.
    frame: u64,
    /// Chunks that stopped drawing while still in render demand: when and why.
    missing_since: BTreeMap<ChunkCoord, (u64, Option<InvalidationCause>)>,
    gaps: GapStats,
    /// What the presentation held at the end of the last update.
    chunk_mesh_gpu_bytes: ChunkMeshGpuBytes,
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
            staged: BTreeMap::new(),
            pending_removal: BTreeSet::new(),
            budget,
            totals: BridgeTotals::default(),
            anchor_rejected: false,
            frame: 0,
            missing_since: BTreeMap::new(),
            gaps: GapStats::default(),
            chunk_mesh_gpu_bytes: ChunkMeshGpuBytes::default(),
        })
    }

    #[must_use]
    pub const fn gaps(&self) -> &GapStats {
        &self.gaps
    }

    /// Records the chunk the camera occupies. A rejected position keeps the
    /// previous center and is counted; it never saturates into a fake chunk.
    pub fn track_camera(&mut self, position: Vec3) -> CameraAnchor {
        match camera_chunk(position) {
            Ok(chunk) => {
                if self.anchor_rejected {
                    self.anchor_rejected = false;
                    warn!(?chunk, "camera anchor is valid again");
                }
                if chunk == self.desired_center {
                    return CameraAnchor::Unchanged;
                }
                self.desired_center = chunk;
                CameraAnchor::Moved(chunk)
            }
            Err(error) => {
                self.totals.anchor_rejections += 1;
                if !self.anchor_rejected {
                    self.anchor_rejected = true;
                    warn!(%error, "camera anchor rejected; keeping previous demand center");
                }
                CameraAnchor::Rejected(error)
            }
        }
    }

    /// One bounded frame step: demand, runtime poll, draw-set reconciliation,
    /// budgeted staging of replacements, atomic group commits, budgeted
    /// releases.
    pub fn update<P: ChunkPresentation>(
        &mut self,
        presentation: &mut P,
    ) -> Result<FrameStreamingReport, RuntimeError> {
        let mut report = FrameStreamingReport::default();
        self.frame += 1;

        if self.desired_center != self.runtime.center() {
            self.runtime.set_demand_center(self.desired_center)?;
            self.totals.demand_updates += 1;
            report.demand_changed = true;
        }
        self.runtime.poll()?;

        report.deactivations = self.reconcile_draw_set(presentation);
        self.discard_obsolete_staging(presentation);
        let (uploads, upload_bytes, deferred, stage_peaks) = self.stage_ready_meshes(presentation);
        self.record_gpu_peaks(stage_peaks);
        report.uploads = uploads;
        report.upload_bytes = upload_bytes;
        report.deferred_uploads = deferred;
        let (commits, committed_chunks, commit_peaks) = self.commit_ready_groups(presentation);
        self.record_gpu_peaks(commit_peaks);
        report.commits = commits;
        report.committed_chunks = committed_chunks;
        report.removals = self.release_pending(presentation);
        self.account_gaps();
        self.sample_chunk_mesh_gpu_bytes(presentation);
        Ok(report)
    }

    /// Records the chunk-mesh bytes the presentation actually holds after this
    /// update. Uploads are additionally observed immediately after every
    /// staging mutation, before an atomic commit can release the old buffers.
    fn sample_chunk_mesh_gpu_bytes<P: ChunkPresentation>(&mut self, presentation: &P) {
        let mut observations = GpuPeakObservations::default();
        let sample = observations.observe(presentation);
        self.record_gpu_peaks(observations);
        self.chunk_mesh_gpu_bytes = sample;
    }

    fn record_gpu_peaks(&mut self, observations: GpuPeakObservations) {
        self.totals.peak_chunk_mesh_committed_bytes = self
            .totals
            .peak_chunk_mesh_committed_bytes
            .max(observations.committed);
        self.totals.peak_staged_bytes = self.totals.peak_staged_bytes.max(observations.staged);
        self.totals.peak_chunk_mesh_total_bytes = self
            .totals
            .peak_chunk_mesh_total_bytes
            .max(observations.total);
    }

    /// Staged replacements whose chunk no longer has a pending target (its
    /// committed mesh became the target again, it left render demand, or it
    /// was dropped) are released; a staged stamp that no longer matches the
    /// pending target is left to be re-staged.
    fn discard_obsolete_staging<P: ChunkPresentation>(&mut self, presentation: &mut P) {
        let obsolete: Vec<ChunkCoord> = self
            .staged
            .keys()
            .copied()
            .filter(|coord| {
                !self.runtime.demand().render.contains(coord)
                    || self.runtime.ready_mesh(*coord).is_none()
            })
            .collect();
        for coord in obsolete {
            self.staged.remove(&coord);
            presentation.discard_staged(coord);
            self.totals.staged_discarded += 1;
        }
    }

    /// Commits every transition group whose members are all CPU-ready and
    /// staged: the runtime commits their meshes and the presentation swaps
    /// them in the same update, so no drawn pair ever mixes old and new seams.
    fn commit_ready_groups<P: ChunkPresentation>(
        &mut self,
        presentation: &mut P,
    ) -> (usize, usize, GpuPeakObservations) {
        let groups = self.runtime.transition_groups();
        let mut commits = 0;
        let mut chunks = 0;
        let mut blocked_groups = 0;
        let mut blocked_group_max = 0;
        let mut constrained_undrawn = 0;
        let mut peak_observations = GpuPeakObservations::default();
        for group in groups {
            let fully_staged = group.iter().all(|coord| {
                match (self.staged.get(coord), self.runtime.ready_mesh(*coord)) {
                    (Some((staged, _)), Some((target, _))) => staged == target,
                    _ => false,
                }
            });
            if !fully_staged || !self.runtime.group_is_ready(&group) {
                blocked_groups += 1;
                blocked_group_max = blocked_group_max.max(group.len());
                constrained_undrawn += self.constrained_undrawn_in(&group);
                continue;
            }
            // Runtime readiness is preflighted while its mutable borrow remains
            // held through the presentation's all-or-nothing swap and the CPU
            // commit. Nothing can invalidate the runtime preflight between the
            // two phases; a refused presentation leaves the runtime untouched.
            let committed = match self
                .runtime
                .commit_group_with(&group, || presentation.commit_staged_group(&group))
            {
                Ok(committed) => committed,
                Err(error) => {
                    self.totals.presentation_commit_failures += 1;
                    warn!(
                        %error,
                        "presentation refused an atomic group commit; the group keeps its current meshes"
                    );
                    blocked_groups += 1;
                    blocked_group_max = blocked_group_max.max(group.len());
                    constrained_undrawn += self.constrained_undrawn_in(&group);
                    continue;
                }
            };
            peak_observations.observe(presentation);
            if committed != group.len() {
                // Both sides verified readiness in this update with nothing in
                // between, so this is unreachable; report it instead of
                // asserting, and let the next reconciliation undraw whatever
                // the runtime does not consider committed.
                self.totals.commit_invariant_failures += 1;
                error!(
                    committed,
                    members = group.len(),
                    "runtime committed fewer chunks than the presentation swapped"
                );
            }
            for coord in &group {
                if let Some((stamp, _)) = self.staged.remove(coord) {
                    self.presented.insert(*coord, stamp);
                    self.pending_removal.remove(coord);
                }
            }
            commits += 1;
            chunks += group.len();
        }
        self.totals.transition_commits += commits as u64;
        self.totals.transition_chunks += chunks as u64;
        self.gaps.blocked_groups_now = blocked_groups;
        self.gaps.blocked_group_max = self.gaps.blocked_group_max.max(blocked_group_max);
        self.gaps.constrained_undrawn_now = constrained_undrawn;
        self.gaps.constrained_undrawn_max =
            self.gaps.constrained_undrawn_max.max(constrained_undrawn);
        (commits, chunks, peak_observations)
    }

    /// Undrawn members of a group that also holds a drawn member: entrants
    /// legitimately waiting for a drawn neighbor whose seam toward them
    /// changes. A group with no drawn member constrains nobody.
    fn constrained_undrawn_in(&self, group: &[ChunkCoord]) -> usize {
        let (drawn, undrawn) = group.iter().fold((0, 0), |(drawn, undrawn), coord| {
            if self.runtime.committed_mesh(*coord).is_some() {
                (drawn + 1, undrawn)
            } else if self
                .runtime
                .ready_mesh(*coord)
                .is_some_and(|(_, mesh)| !mesh.indices().is_empty())
            {
                (drawn, undrawn + 1)
            } else {
                (drawn, undrawn)
            }
        });
        if drawn > 0 { undrawn } else { 0 }
    }

    /// Closes gaps for chunks that draw again, forgets chunks that left render
    /// demand, and samples the number still missing this frame.
    fn account_gaps(&mut self) {
        let render = &self.runtime.demand().render;
        let frame = self.frame;
        let mut closed = Vec::new();
        self.missing_since.retain(|coord, (since, cause)| {
            if !render.contains(coord) {
                return false;
            }
            if self.presented.contains_key(coord) {
                closed.push((frame - *since, *cause));
                return false;
            }
            true
        });
        for (length, cause) in closed {
            match cause {
                Some(InvalidationCause::LodChange) => self.gaps.lod_gaps += 1,
                Some(InvalidationCause::NeighborPresentation) => {
                    self.gaps.neighbor_presentation_gaps += 1;
                }
                Some(InvalidationCause::Membership) => self.gaps.membership_gaps += 1,
                Some(InvalidationCause::Data) => self.gaps.data_gaps += 1,
                None => self.gaps.unattributed_gaps += 1,
            }
            self.gaps.gap_frames_total += length;
            self.gaps.max_gap_frames = self.gaps.max_gap_frames.max(length);
        }
        self.gaps.current_missing = self.missing_since.len();
        self.gaps.max_simultaneous_missing = self
            .gaps
            .max_simultaneous_missing
            .max(self.gaps.current_missing);
        if self.gaps.current_missing > 0 {
            self.gaps.frames_with_missing += 1;
        }
        let mut ready_awaiting_upload = 0;
        let mut ready_blocked_transition = 0;
        for (coord, stamp, mesh) in self.runtime.render_ready_meshes() {
            match classify_ready_undrawn(
                self.presented.contains_key(&coord),
                mesh.indices().is_empty(),
                self.staged
                    .get(&coord)
                    .is_some_and(|(staged, _)| staged == stamp),
            ) {
                Some(ReadyUndrawnReason::AwaitingUpload) => ready_awaiting_upload += 1,
                Some(ReadyUndrawnReason::BlockedByTransition) => {
                    ready_blocked_transition += 1;
                }
                None => {}
            }
        }
        let ready_undrawn = ready_awaiting_upload + ready_blocked_transition;
        self.gaps.ready_undrawn_now = ready_undrawn;
        self.gaps.ready_undrawn_max = self.gaps.ready_undrawn_max.max(ready_undrawn);
        if ready_undrawn > 0 {
            self.gaps.ready_undrawn_frames += 1;
            self.gaps.ready_undrawn_chunk_frames += ready_undrawn as u64;
        }
        self.gaps.ready_awaiting_upload_now = ready_awaiting_upload;
        self.gaps.ready_awaiting_upload_max = self
            .gaps
            .ready_awaiting_upload_max
            .max(ready_awaiting_upload);
        self.gaps.ready_blocked_transition_now = ready_blocked_transition;
        self.gaps.ready_blocked_transition_max = self
            .gaps
            .ready_blocked_transition_max
            .max(ready_blocked_transition);

        let committed_missing = self
            .runtime
            .render_committed_meshes()
            .filter(|(coord, stamp, mesh)| {
                !mesh.indices().is_empty() && self.presented.get(coord) != Some(*stamp)
            })
            .count();
        self.gaps.committed_missing_now = committed_missing;
        self.gaps.committed_missing_max = self.gaps.committed_missing_max.max(committed_missing);

        let frontier_pending = render
            .iter()
            .filter(|coord| {
                !self.presented.contains_key(coord)
                    && !matches!(
                        self.runtime.residency_status(**coord),
                        Some(veldwake_streaming::ResidencyStatus::KnownAbsent)
                    )
                    && self.runtime.ready_mesh(**coord).is_none()
                    && self.runtime.committed_mesh(**coord).is_none()
            })
            .count();
        self.gaps.frontier_pipeline_pending_now = frontier_pending;
        self.gaps.frontier_pipeline_pending_max = self
            .gaps
            .frontier_pipeline_pending_max
            .max(frontier_pending);
    }

    /// Any presented mesh that the runtime no longer commits (a data change,
    /// an unload, or leaving render demand) stops drawing now, regardless of
    /// release budget. A presentation-only target change keeps it drawn until
    /// its transition group commits.
    fn reconcile_draw_set<P: ChunkPresentation>(&mut self, presentation: &mut P) -> usize {
        let render = &self.runtime.demand().render;
        let stale: Vec<ChunkCoord> = self
            .presented
            .iter()
            .filter(|(coord, stamp)| {
                !render.contains(*coord)
                    || self
                        .runtime
                        .committed_mesh(**coord)
                        .is_none_or(|(current, _)| current != *stamp)
            })
            .map(|(coord, _)| *coord)
            .collect();
        for coord in &stale {
            self.presented.remove(coord);
            if render.contains(coord) {
                let cause = self.runtime.last_invalidation(*coord);
                self.missing_since
                    .entry(*coord)
                    .or_insert((self.frame, cause));
            }
            // An empty mesh was presented without buffers; only a chunk that
            // actually held GPU state needs a budgeted release.
            if presentation.deactivate_chunk(*coord) {
                self.pending_removal.insert(*coord);
            }
        }
        self.totals.deactivations += stale.len() as u64;
        stale.len()
    }

    /// Stages (uploads without drawing) every pending replacement whose
    /// target stamp is not staged yet, nearest first, under the upload budget.
    fn stage_ready_meshes<P: ChunkPresentation>(
        &mut self,
        presentation: &mut P,
    ) -> (usize, usize, usize, GpuPeakObservations) {
        let center = self.runtime.center();
        let mut candidates: Vec<(u128, ChunkCoord, MeshStamp, &Mesh)> = self
            .runtime
            .render_ready_meshes()
            .filter(|(coord, stamp, _)| {
                self.staged.get(coord).map(|(staged, _)| staged) != Some(*stamp)
            })
            .map(|(coord, stamp, mesh)| (squared_distance(coord, center), coord, *stamp, mesh))
            .collect();
        candidates.sort_by_key(|(distance, coord, _, _)| (*distance, *coord));

        let mut uploads = 0;
        let mut bytes_done = 0;
        let mut deferred = 0;
        let mut staged_now = Vec::new();
        let mut peak_observations = GpuPeakObservations::default();
        for (_, coord, stamp, mesh) in candidates {
            let restage = self.staged.contains_key(&coord);
            if mesh.indices().is_empty() {
                // Nothing reaches the GPU; committing it later only clears buffers.
                match presentation.stage_chunk(coord, stamp.lod, mesh) {
                    Ok(_) => {
                        self.totals.empty_meshes_presented += 1;
                        staged_now.push((coord, stamp, 0, restage));
                        peak_observations.observe(presentation);
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
            match presentation.stage_chunk(coord, stamp.lod, mesh) {
                Ok(uploaded) => {
                    uploads += 1;
                    bytes_done += uploaded;
                    match stamp.lod {
                        LodLevel::Lod0 => {
                            self.totals.uploads_lod0 += 1;
                            self.totals.upload_bytes_lod0 += uploaded as u64;
                        }
                        LodLevel::Lod1 => {
                            self.totals.uploads_lod1 += 1;
                            self.totals.upload_bytes_lod1 += uploaded as u64;
                        }
                    }
                    staged_now.push((coord, stamp, uploaded, restage));
                    // Observe before the later group commit can release the
                    // old active buffers. Sampling only at update end misses
                    // this transient but real overlap.
                    peak_observations.observe(presentation);
                }
                Err(error) => {
                    self.totals.upload_failures += 1;
                    warn!(%error, "chunk mesh upload failed; chunk stays undrawn");
                }
            }
        }
        for (coord, stamp, bytes, restage) in staged_now {
            self.staged.insert(coord, (stamp, bytes));
            if restage {
                self.totals.restaged += 1;
            }
        }
        self.totals.uploads += uploads as u64;
        self.totals.upload_bytes += bytes_done as u64;
        self.totals.upload_budget_deferrals += deferred as u64;
        (uploads, bytes_done, deferred, peak_observations)
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

    #[must_use]
    pub fn staged_count(&self) -> usize {
        self.staged.len()
    }

    #[must_use]
    pub fn staged_bytes(&self) -> usize {
        self.staged.values().map(|(_, bytes)| *bytes).sum()
    }

    /// GPU bytes the presentation held at the end of the last update.
    #[must_use]
    pub const fn chunk_mesh_gpu_bytes(&self) -> ChunkMeshGpuBytes {
        self.chunk_mesh_gpu_bytes
    }

    /// Chunks the presentation is drawing, with the stamp each was committed
    /// for. The debug visualization reads presentation truth from here.
    pub fn presented(&self) -> impl Iterator<Item = (ChunkCoord, &MeshStamp)> + '_ {
        self.presented.iter().map(|(coord, stamp)| (*coord, stamp))
    }

    /// Whether a replacement for `coord` is uploaded but not drawn yet.
    #[must_use]
    pub fn is_staged(&self, coord: ChunkCoord) -> bool {
        self.staged.contains_key(&coord)
    }

    #[cfg(test)]
    pub fn presented_stamp(&self, coord: ChunkCoord) -> Option<&MeshStamp> {
        self.presented.get(&coord)
    }

    #[cfg(test)]
    pub fn staged_coords(&self) -> Vec<ChunkCoord> {
        self.staged.keys().copied().collect()
    }

    #[cfg(test)]
    pub fn staged_stamp(&self, coord: ChunkCoord) -> Option<&MeshStamp> {
        self.staged.get(&coord).map(|(stamp, _)| stamp)
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
pub(crate) mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        thread,
        time::Duration,
    };

    use glam::Vec3;
    use veldwake_streaming::{
        LodLevel, MeshStamp, NeighborPresentation, NeighborStamp, SeamContract, StreamingConfig,
    };
    use veldwake_voxel::{CHUNK_EDGE, Chunk, ChunkCoord, Face, Mesh, WorldVoxelCoord};

    use super::{
        CameraAnchor, CameraAnchorError, ChunkPresentation, ChunkUploadError, GpuPeakObservations,
        GpuResidency, PresentationCommitError, ReadyUndrawnReason, StreamingBridge,
        UploadAdmission, UploadBudget, camera_chunk, camera_world_voxel, classify_ready_undrawn,
    };

    /// One held mesh in the double: bytes, quads, level, drawable.
    #[derive(Clone, Copy, Debug)]
    struct Held {
        bytes: usize,
        quads: usize,
        lod: LodLevel,
        active: bool,
    }

    #[derive(Default)]
    pub(crate) struct FakePresentation {
        chunks: BTreeMap<ChunkCoord, Held>,
        /// Staged replacements: `None` marks a staged empty mesh.
        staged: BTreeMap<ChunkCoord, Option<Held>>,
        upserts: usize,
        removals: usize,
        /// Every level the bridge sent, in order, including empty meshes.
        levels_sent: Vec<(ChunkCoord, LodLevel)>,
        /// Group commits refused because a member had nothing staged.
        refused_groups: usize,
        /// Independent mutation-boundary oracle for chunk-mesh byte peaks.
        observed_peak: GpuPeakObservations,
        /// Restages that released an obsolete replacement before acquiring the
        /// new one, as the ordering contract requires.
        released_before_restage: usize,
        /// Makes the next `stage_chunk` reject before touching any state, the
        /// way the renderer rejects an unrepresentable index count before it
        /// releases or allocates anything.
        reject_next_stage: bool,
    }

    impl FakePresentation {
        /// Drops a staged replacement behind the bridge's back, simulating a
        /// presentation that lost GPU state the bridge believes is staged.
        pub(crate) fn lose_staging(&mut self, coord: ChunkCoord) -> bool {
            let removed = self.staged.remove(&coord).is_some();
            self.observe_bytes();
            removed
        }

        /// Held bytes of the staged replacement of `coord`, if any.
        fn staged_bytes_of(&self, coord: ChunkCoord) -> Option<usize> {
            self.staged
                .get(&coord)
                .map(|held| held.map_or(0, |held| held.bytes))
        }

        /// Held bytes of the committed mesh of `coord`, if any.
        fn committed_bytes_of(&self, coord: ChunkCoord) -> Option<usize> {
            self.chunks.get(&coord).map(|held| held.bytes)
        }

        const fn released_before_restage(&self) -> usize {
            self.released_before_restage
        }

        const fn reject_next_stage(&mut self) {
            self.reject_next_stage = true;
        }

        const fn observed_peak(&self) -> GpuPeakObservations {
            self.observed_peak
        }

        fn observe_bytes(&mut self) {
            let residency = self.residency();
            self.observed_peak.committed = self.observed_peak.committed.max(residency.bytes());
            self.observed_peak.staged = self.observed_peak.staged.max(residency.staged_bytes());
            self.observed_peak.total = self
                .observed_peak
                .total
                .max(residency.bytes() + residency.staged_bytes());
        }
    }

    impl ChunkPresentation for FakePresentation {
        fn gpu_payload_bytes(&self, mesh: &Mesh) -> usize {
            mesh.vertices().len() * 24 + mesh.indices().len() * 4 + 32
        }

        fn stage_chunk(
            &mut self,
            coord: ChunkCoord,
            lod: LodLevel,
            mesh: &Mesh,
        ) -> Result<usize, ChunkUploadError> {
            // Rejection comes first, before any state is touched.
            if self.reject_next_stage {
                self.reject_next_stage = false;
                return Err(ChunkUploadError::TooManyIndices {
                    coord,
                    indices: mesh.indices().len(),
                });
            }
            self.upserts += 1;
            self.levels_sent.push((coord, lod));
            if mesh.indices().is_empty() {
                self.staged.insert(coord, None);
                self.observe_bytes();
                return Ok(0);
            }
            let bytes = self.gpu_payload_bytes(mesh);
            // Release the obsolete replacement before accounting for its
            // successor, mirroring the renderer so the peak oracle sees the
            // same instants the real presentation passes through.
            if self.staged.remove(&coord).is_some() {
                self.released_before_restage += 1;
                self.observe_bytes();
            }
            self.staged.insert(
                coord,
                Some(Held {
                    bytes,
                    quads: mesh.quad_count(),
                    lod,
                    active: true,
                }),
            );
            self.observe_bytes();
            Ok(bytes)
        }

        fn commit_staged_group(
            &mut self,
            group: &[ChunkCoord],
        ) -> Result<(), PresentationCommitError> {
            if let Some(missing) = group.iter().find(|coord| !self.staged.contains_key(coord)) {
                self.refused_groups += 1;
                return Err(PresentationCommitError {
                    coord: *missing,
                    group_size: group.len(),
                });
            }
            for coord in group {
                match self.staged.remove(coord) {
                    Some(Some(held)) => {
                        self.chunks.insert(*coord, held);
                    }
                    Some(None) | None => {
                        self.chunks.remove(coord);
                    }
                }
            }
            self.observe_bytes();
            Ok(())
        }

        fn discard_staged(&mut self, coord: ChunkCoord) -> bool {
            let removed = self.staged.remove(&coord).is_some();
            self.observe_bytes();
            removed
        }

        fn deactivate_chunk(&mut self, coord: ChunkCoord) -> bool {
            let deactivated = self
                .chunks
                .get_mut(&coord)
                .map(|held| held.active = false)
                .is_some();
            self.observe_bytes();
            deactivated
        }

        fn remove_chunk(&mut self, coord: ChunkCoord) -> bool {
            self.removals += 1;
            self.staged.remove(&coord);
            let removed = self.chunks.remove(&coord).is_some();
            self.observe_bytes();
            removed
        }

        fn residency(&self) -> GpuResidency {
            let mut residency = GpuResidency::default();
            for held in self.chunks.values() {
                let level = match held.lod {
                    LodLevel::Lod0 => &mut residency.lod0,
                    LodLevel::Lod1 => &mut residency.lod1,
                };
                level.resident += 1;
                level.bytes += held.bytes;
                level.quads += held.quads;
                if held.active {
                    level.active += 1;
                }
            }
            for held in self.staged.values().flatten() {
                let level = match held.lod {
                    LodLevel::Lod0 => &mut residency.lod0,
                    LodLevel::Lod1 => &mut residency.lod1,
                };
                level.staged += 1;
                level.staged_bytes += held.bytes;
            }
            residency
        }
    }

    /// Every drawn pair must agree on its seam: each presented stamp's
    /// contract toward a presented neighbor equals the contract derived from
    /// the neighbor's presented level. Toward an undrawn neighbor still in
    /// render demand it equals the contract for that neighbor's target level,
    /// so the pair is coherent the moment the neighbor appears, unless the
    /// drawn chunk's own replacement toward that face is pending: it then
    /// keeps its old mesh and commits together with the neighbor. This is the
    /// definition of "no mixed committed/target seam".
    fn assert_presented_seams_coherent(bridge: &StreamingBridge, fake: &FakePresentation) {
        let presented: BTreeMap<ChunkCoord, MeshStamp> = bridge
            .runtime()
            .render_committed_meshes()
            .filter(|(coord, _, _)| bridge.presented_stamp(*coord).is_some())
            .map(|(coord, stamp, _)| (coord, *stamp))
            .collect();
        for (coord, stamp) in &presented {
            assert_eq!(bridge.presented_stamp(*coord), Some(stamp), "{coord:?}");
            for face in Face::ALL {
                let Some(neighbor) = coord.neighbor(face) else {
                    continue;
                };
                let NeighborStamp::Resident { seam, .. } = stamp.neighbors[face as usize] else {
                    continue;
                };
                let expected = match presented.get(&neighbor) {
                    Some(other) => {
                        SeamContract::derive(stamp.lod, NeighborPresentation::Rendered(other.lod))
                    }
                    // A neighbor that left render demand is beyond the visible
                    // radius: the committed mesh may still carry its old contract
                    // until its own transition commits (boundary edge effect).
                    None if !bridge.runtime().demand().render.contains(&neighbor) => continue,
                    None if bridge.runtime().seam_changes(*coord, face) => continue,
                    None => match bridge.runtime().desired_lod(neighbor) {
                        Some(level) => {
                            SeamContract::derive(stamp.lod, NeighborPresentation::Rendered(level))
                        }
                        None => SeamContract::Same,
                    },
                };
                assert_eq!(
                    seam, expected,
                    "{coord:?} toward {face:?} ({neighbor:?}) drawn with an incompatible seam"
                );
            }
        }
        // The double's draw set is exactly the presented non-empty set.
        for coord in fake.active() {
            assert!(
                presented.contains_key(&coord),
                "{coord:?} drawn but not committed"
            );
        }
    }

    impl FakePresentation {
        pub(crate) fn active(&self) -> Vec<ChunkCoord> {
            self.chunks
                .iter()
                .filter(|(_, held)| held.active)
                .map(|(coord, _)| *coord)
                .collect()
        }
    }

    pub(crate) fn settle(bridge: &mut StreamingBridge, fake: &mut FakePresentation) {
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
    fn coverage_metrics_separate_frontier_pipeline_from_ready_upload_wait() {
        assert_eq!(
            classify_ready_undrawn(false, false, false),
            Some(ReadyUndrawnReason::AwaitingUpload)
        );
        assert_eq!(
            classify_ready_undrawn(false, false, true),
            Some(ReadyUndrawnReason::BlockedByTransition)
        );
        assert_eq!(classify_ready_undrawn(true, false, true), None);
        assert_eq!(classify_ready_undrawn(false, true, true), None);

        let budget = UploadBudget {
            max_uploads_per_frame: 0,
            soft_bytes_per_frame: 0,
            max_removals_per_frame: 8,
        };
        let mut fake = FakePresentation::default();
        let mut bridge = bridge_at(Vec3::new(5.0, 5.0, 5.0), budget);

        bridge.account_gaps();
        assert!(bridge.gaps().frontier_pipeline_pending_now > 0);
        assert_eq!(bridge.gaps().ready_undrawn_now, 0);

        for _ in 0..20_000 {
            if let Err(error) = bridge.update(&mut fake) {
                panic!("streaming update failed: {error}");
            }
            if bridge.runtime().is_idle() && bridge.gaps().ready_awaiting_upload_now > 0 {
                break;
            }
            thread::sleep(Duration::from_micros(50));
        }
        assert!(bridge.gaps().ready_awaiting_upload_now > 0);
        assert_eq!(bridge.gaps().ready_blocked_transition_now, 0);
        assert_eq!(
            bridge.gaps().ready_undrawn_now,
            bridge.gaps().ready_awaiting_upload_now
        );
        assert_eq!(bridge.gaps().committed_missing_now, 0);
    }

    #[test]
    fn camera_tracking_only_changes_center_across_chunk_boundaries() {
        let mut bridge = bridge_at(Vec3::new(5.0, 5.0, 5.0), UploadBudget::default());
        assert_eq!(bridge.desired_center(), ChunkCoord::new(0, 0, 0));
        assert_eq!(
            bridge.track_camera(Vec3::new(31.9, 5.0, 5.0)),
            CameraAnchor::Unchanged
        );
        assert_eq!(
            bridge.track_camera(Vec3::new(32.0, 5.0, 5.0)),
            CameraAnchor::Moved(ChunkCoord::new(1, 0, 0))
        );
        assert_eq!(bridge.desired_center(), ChunkCoord::new(1, 0, 0));
        assert_eq!(
            bridge.track_camera(Vec3::new(-0.001, 5.0, 5.0)),
            CameraAnchor::Moved(ChunkCoord::new(-1, 0, 0))
        );
        assert_eq!(bridge.desired_center(), ChunkCoord::new(-1, 0, 0));

        let rejected = bridge.track_camera(Vec3::new(f32::NAN, 5.0, 5.0));
        assert!(matches!(
            rejected,
            CameraAnchor::Rejected(CameraAnchorError::NonFinite { .. })
        ));
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
        assert_eq!(
            bridge.runtime().render_ready_meshes().count(),
            0,
            "all committed"
        );
        let committed: Vec<_> = bridge.runtime().render_committed_meshes().collect();
        assert_eq!(committed.len(), render.len());
        for (coord, stamp, mesh) in committed {
            assert!(render.contains(&coord));
            assert_eq!(bridge.presented_stamp(coord), Some(stamp));
            let held = fake.chunks.get(&coord);
            if mesh.indices().is_empty() {
                assert!(held.is_none(), "empty mesh must hold no GPU bytes");
            } else {
                assert!(
                    matches!(
                        held,
                        Some(Held {
                            active: true,
                            lod: LodLevel::Lod0,
                            ..
                        })
                    ),
                    "non-empty mesh must be active at Lod0"
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
        let held_before = fake.residency().resident();
        assert!(
            held_before > 2,
            "need a backlog larger than one release batch"
        );

        let empty_presented = bridge.presented_count() - held_before;
        assert!(
            empty_presented > 0,
            "the corridor's air layer yields empty meshes"
        );

        assert!(matches!(
            bridge.track_camera(Vec3::new(400.0, 5.0, 400.0)),
            CameraAnchor::Moved(_)
        ));
        let report = match bridge.update(&mut fake) {
            Ok(report) => report,
            Err(error) => panic!("update failed: {error}"),
        };
        assert!(report.demand_changed);
        assert_eq!(
            fake.residency().active(),
            0,
            "stale meshes must stop drawing now"
        );
        assert_eq!(bridge.presented_count(), 0);
        assert_eq!(report.removals, 2);
        assert!(
            fake.residency().resident() > 0,
            "release is budgeted, not immediate"
        );
        assert_eq!(bridge.totals().removal_budget_hits, 1);
        // Only chunks that held buffers queue for release; empty ones never did.
        assert_eq!(
            bridge.pending_removal_count() + report.removals,
            held_before,
            "empty presented meshes must not consume release budget"
        );

        settle(&mut bridge, &mut fake);
        assert_eq!(fake.residency().resident(), 0);
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

        assert!(matches!(
            bridge.track_camera(Vec3::new(400.0, 5.0, 400.0)),
            CameraAnchor::Moved(_)
        ));
        settle(&mut bridge, &mut fake);
        assert_eq!(bridge.presented_stamp(origin), None);

        assert!(matches!(
            bridge.track_camera(Vec3::new(5.0, 5.0, 5.0)),
            CameraAnchor::Moved(_)
        ));
        settle(&mut bridge, &mut fake);
        let second = *bridge
            .presented_stamp(origin)
            .unwrap_or_else(|| panic!("origin not re-presented"));
        assert_ne!(first.request_token, second.request_token);
        assert!(matches!(
            fake.chunks.get(&origin),
            Some(Held { active: true, .. })
        ));
    }

    #[test]
    fn banded_profile_presents_both_levels_with_per_level_accounting() {
        let mut fake = FakePresentation::default();
        let mut bridge = match StreamingBridge::new(
            StreamingConfig::m3c_diagnostic(),
            UploadBudget::default(),
            Vec3::new(5.0, 5.0, 5.0),
        ) {
            Ok(bridge) => bridge,
            Err(error) => panic!("bridge failed to start: {error}"),
        };
        settle(&mut bridge, &mut fake);

        let summary = bridge.runtime().summary();
        assert!(summary.lod1_committed > 0 && summary.lod0_committed > 0);
        assert_eq!(
            summary.lod0_ready + summary.lod1_ready,
            0,
            "everything committed"
        );
        let residency = fake.residency();
        assert!(residency.lod0.resident > 0 && residency.lod1.resident > 0);
        assert_eq!(
            residency.resident(),
            residency.lod0.resident + residency.lod1.resident
        );
        assert_eq!(
            residency.active(),
            residency.resident(),
            "all presented meshes drawable"
        );
        assert_eq!(
            residency.bytes(),
            residency.lod0.bytes + residency.lod1.bytes
        );
        assert!(residency.quads() > 0);
        // The level sent to the presentation matches the committed mesh's stamp.
        for (coord, stamp, mesh) in bridge.runtime().render_committed_meshes() {
            if mesh.indices().is_empty() {
                continue;
            }
            let held = fake
                .chunks
                .get(&coord)
                .unwrap_or_else(|| panic!("{coord:?} not held"));
            assert_eq!(held.lod, stamp.lod, "{coord:?}");
        }
        assert!(
            fake.levels_sent
                .iter()
                .any(|(_, lod)| *lod == LodLevel::Lod1)
        );
        let totals = bridge.totals();
        assert_eq!(totals.uploads, totals.uploads_lod0 + totals.uploads_lod1);
        assert_eq!(
            totals.upload_bytes,
            totals.upload_bytes_lod0 + totals.upload_bytes_lod1
        );
        assert!(totals.uploads_lod1 > 0);
        assert_eq!(
            residency.bytes() as u64,
            totals.upload_bytes,
            "nothing re-uploaded"
        );
        assert_presented_seams_coherent(&bridge, &fake);
    }

    /// Drives one camera move from a settled state and proves that no chunk
    /// which stayed in render demand and was drawn before the move is undrawn
    /// in any update, and that every update's drawn set is seam-coherent.
    fn move_without_gaps(
        bridge: &mut StreamingBridge,
        fake: &mut FakePresentation,
        position: Vec3,
    ) -> usize {
        let before: Vec<ChunkCoord> = fake.active();
        assert!(matches!(
            bridge.track_camera(position),
            CameraAnchor::Moved(_)
        ));
        let mut commits = 0;
        for _ in 0..20_000 {
            let report = match bridge.update(fake) {
                Ok(report) => report,
                Err(error) => panic!("update failed: {error}"),
            };
            commits += report.commits;
            let render = &bridge.runtime().demand().render;
            let active: BTreeSet<ChunkCoord> = fake.active().into_iter().collect();
            for coord in before.iter().filter(|coord| render.contains(coord)) {
                assert!(
                    active.contains(coord),
                    "{coord:?} lost its mesh during a level transition"
                );
            }
            assert_presented_seams_coherent(bridge, fake);
            let quiet = report.uploads == 0
                && report.deactivations == 0
                && report.removals == 0
                && report.deferred_uploads == 0
                && report.commits == 0;
            if quiet && bridge.runtime().is_idle() && bridge.pending_removal_count() == 0 {
                break;
            }
            thread::sleep(Duration::from_micros(50));
        }
        assert!(bridge.runtime().is_idle(), "did not settle");
        assert_eq!(
            bridge.gaps().closed_gaps(),
            0,
            "a gap opened: {:?}",
            bridge.gaps()
        );
        assert_eq!(bridge.runtime().render_ready_meshes().count(), 0);
        commits
    }

    /// One solid voxel meshed on its own: enough to occupy presentation slots.
    fn single_voxel_mesh() -> Mesh {
        voxel_mesh(&[(0, 0, 0)])
    }

    /// A mesh of isolated voxels: every extra voxel adds six quads, so two
    /// calls with different counts give reliably different payload sizes.
    fn voxel_mesh(voxels: &[(usize, usize, usize)]) -> Mesh {
        let mut chunk = Chunk::empty();
        for &(x, y, z) in voxels {
            let local = match veldwake_voxel::LocalCoord::new(x, y, z) {
                Ok(local) => local,
                Err(error) => panic!("local coordinate rejected: {error}"),
            };
            let previous = chunk.write_local(local, veldwake_voxel::VoxelId(1));
            assert!(previous.is_air());
        }
        veldwake_voxel::mesh_exposed_faces(&chunk)
    }

    pub(crate) fn banded_bridge_at(position: Vec3) -> StreamingBridge {
        match StreamingBridge::new(
            StreamingConfig::m3c_diagnostic(),
            UploadBudget::default(),
            position,
        ) {
            Ok(bridge) => bridge,
            Err(error) => panic!("bridge failed to start: {error}"),
        }
    }

    #[test]
    fn level_transitions_in_every_axial_direction_open_no_gap() {
        // Start inside the corridor so both levels and both signs are exercised.
        let start = Vec3::new(5.0, 5.0, 5.0);
        let steps: [(Vec3, &str); 6] = [
            (Vec3::new(37.0, 5.0, 5.0), "+x"),
            (Vec3::new(-27.0, 5.0, 5.0), "-x"),
            (Vec3::new(5.0, 37.0, 5.0), "+y"),
            (Vec3::new(5.0, -27.0, 5.0), "-y"),
            (Vec3::new(5.0, 5.0, 37.0), "+z"),
            (Vec3::new(5.0, 5.0, -27.0), "-z"),
        ];
        for (target, name) in steps {
            let mut fake = FakePresentation::default();
            let mut bridge = banded_bridge_at(start);
            settle(&mut bridge, &mut fake);
            let swaps_before = bridge.runtime().metrics().lod_swaps;
            let commits = move_without_gaps(&mut bridge, &mut fake, target);
            assert!(
                bridge.runtime().metrics().lod_swaps > swaps_before,
                "{name}: the move must swap levels"
            );
            // Vertical moves swap levels only on source-absent layers, which
            // have no mesh to transition; horizontal moves must commit groups.
            if name.ends_with('x') || name.ends_with('z') {
                assert!(
                    commits > 0,
                    "{name}: swaps must commit through transition groups"
                );
            }
            // Back again: Lod1 -> Lod0 promotions and Lod0 -> Lod1 demotions reversed.
            move_without_gaps(&mut bridge, &mut fake, start);
            assert!(bridge.totals().transition_commits > 0, "{name}");
        }
    }

    /// Continuous diagonal movement under pipeline pressure: a bounded number
    /// of updates per step, never settling, so the frontier keeps entering
    /// while the Lod0 core keeps re-dirtying its ring. A drawn chunk that stays
    /// in render demand keeps drawing, the drawn set stays seam-coherent, and
    /// an undrawn chunk waits on a drawn neighbor only when that neighbor's
    /// seam toward it changes; joining every dirty adjacency chained entrants
    /// to the ring and left the floor missing for seconds (KI-009).
    #[test]
    fn continuous_diagonal_movement_never_starves_unconstrained_entrants() {
        let mut fake = FakePresentation::default();
        let start = Vec3::new(-120.0, 40.0, -120.0);
        let mut bridge = banded_bridge_at(start);
        settle(&mut bridge, &mut fake);
        let mut previous: BTreeSet<ChunkCoord> = fake.active().into_iter().collect();
        for step in 1..=60 {
            let offset = step as f32 * 4.0;
            bridge.track_camera(start + Vec3::new(offset, 0.0, offset));
            for _ in 0..8 {
                if let Err(error) = bridge.update(&mut fake) {
                    panic!("update failed: {error}");
                }
                let runtime = bridge.runtime();
                let render = &runtime.demand().render;
                let active: BTreeSet<ChunkCoord> = fake.active().into_iter().collect();
                for coord in previous.iter().filter(|coord| render.contains(coord)) {
                    assert!(
                        active.contains(coord),
                        "{coord:?} vanished while still in render demand"
                    );
                }
                assert_presented_seams_coherent(&bridge, &fake);
                for group in runtime.transition_groups() {
                    let any_drawn = group
                        .iter()
                        .any(|&other| runtime.committed_mesh(other).is_some());
                    for &member in &group {
                        if !any_drawn || runtime.committed_mesh(member).is_some() {
                            continue;
                        }
                        let justified = Face::ALL.iter().any(|&face| {
                            member.neighbor(face).is_some_and(|neighbor| {
                                group.contains(&neighbor)
                                    && runtime.seam_changes(neighbor, face.opposite())
                            })
                        });
                        assert!(
                            justified,
                            "{member:?} waits on drawn chunks whose seams toward it do not change: {group:?}"
                        );
                    }
                }
                previous = active;
                thread::sleep(Duration::from_micros(50));
            }
        }
        settle(&mut bridge, &mut fake);
        assert_eq!(bridge.gaps().closed_gaps(), 0, "{:?}", bridge.gaps());
        assert_presented_seams_coherent(&bridge, &fake);
    }

    #[test]
    fn repeated_swaps_on_the_same_crossing_open_no_gap() {
        let mut fake = FakePresentation::default();
        let mut bridge = banded_bridge_at(Vec3::new(5.0, 5.0, 5.0));
        settle(&mut bridge, &mut fake);
        for _ in 0..3 {
            move_without_gaps(&mut bridge, &mut fake, Vec3::new(37.0, 5.0, 5.0));
            move_without_gaps(&mut bridge, &mut fake, Vec3::new(5.0, 5.0, 5.0));
        }
        assert!(bridge.totals().transition_commits > 0);
    }

    #[test]
    fn camera_change_before_commit_coalesces_the_target_and_never_activates_stale_staging() {
        let mut fake = FakePresentation::default();
        let mut bridge = banded_bridge_at(Vec3::new(5.0, 5.0, 5.0));
        settle(&mut bridge, &mut fake);
        let before: Vec<ChunkCoord> = fake.active();

        // First move: run only a few updates so replacements are in flight.
        assert!(matches!(
            bridge.track_camera(Vec3::new(37.0, 5.0, 5.0)),
            CameraAnchor::Moved(_)
        ));
        for _ in 0..40 {
            if let Err(error) = bridge.update(&mut fake) {
                panic!("update failed: {error}");
            }
            thread::sleep(Duration::from_micros(200));
        }
        assert!(
            bridge.runtime().summary().transition_pending > 0 || bridge.staged_count() > 0,
            "the first move must still be in transition"
        );
        // Second move before the first commits: the old target is abandoned.
        assert!(matches!(
            bridge.track_camera(Vec3::new(69.0, 5.0, 5.0)),
            CameraAnchor::Moved(_)
        ));
        for _ in 0..20_000 {
            let report = match bridge.update(&mut fake) {
                Ok(report) => report,
                Err(error) => panic!("update failed: {error}"),
            };
            let render = &bridge.runtime().demand().render;
            let active: BTreeSet<ChunkCoord> = fake.active().into_iter().collect();
            for coord in before.iter().filter(|coord| render.contains(coord)) {
                assert!(
                    active.contains(coord),
                    "{coord:?} lost its mesh across coalesced moves"
                );
            }
            assert_presented_seams_coherent(&bridge, &fake);
            // Whatever is presented is exactly what the runtime commits: a stale
            // staged replacement can never become drawable.
            for (coord, stamp) in bridge
                .runtime()
                .render_committed_meshes()
                .map(|(c, s, _)| (c, *s))
            {
                if let Some(presented) = bridge.presented_stamp(coord) {
                    assert_eq!(
                        *presented, stamp,
                        "{coord:?} draws a stamp the runtime did not commit"
                    );
                }
            }
            if report.uploads == 0
                && report.commits == 0
                && report.deactivations == 0
                && bridge.runtime().is_idle()
                && bridge.pending_removal_count() == 0
            {
                break;
            }
            thread::sleep(Duration::from_micros(50));
        }
        assert!(bridge.runtime().is_idle());
        assert_eq!(bridge.gaps().closed_gaps(), 0, "{:?}", bridge.gaps());
        assert_eq!(bridge.runtime().center(), ChunkCoord::new(2, 0, 0));
        assert!(
            bridge.totals().restaged > 0 || bridge.totals().staged_discarded > 0,
            "the abandoned target must have been coalesced away"
        );
        assert_presented_seams_coherent(&bridge, &fake);
    }

    /// The chunk-mesh byte peak includes transient staging before same-update
    /// commits; end-of-update sampling alone provably misses that overlap.
    #[test]
    fn the_gpu_byte_peak_is_the_highest_simultaneous_total() {
        let mut fake = FakePresentation::default();
        let budget = UploadBudget {
            max_uploads_per_frame: usize::MAX,
            soft_bytes_per_frame: usize::MAX,
            max_removals_per_frame: usize::MAX,
        };
        let mut bridge = match StreamingBridge::new(
            StreamingConfig::m3c_diagnostic(),
            budget,
            Vec3::new(5.0, 5.0, 5.0),
        ) {
            Ok(bridge) => bridge,
            Err(error) => panic!("bridge failed to start: {error}"),
        };
        settle(&mut bridge, &mut fake);

        // Prepare every replacement before allowing the bridge to upload. One
        // update must therefore hold all old committed buffers and all staged
        // replacements before its atomic commits release the old set.
        let target = ChunkCoord::new(1, 0, 0);
        if let Err(error) = bridge.runtime_mut().set_demand_center(target) {
            panic!("demand move failed: {error}");
        }
        assert!(matches!(
            bridge.track_camera(Vec3::new(37.0, 5.0, 5.0)),
            CameraAnchor::Moved(coord) if coord == target
        ));
        for _ in 0..40_000 {
            if let Err(error) = bridge.runtime_mut().poll() {
                panic!("streaming poll failed: {error}");
            }
            if bridge.runtime().is_idle() {
                break;
            }
            thread::sleep(Duration::from_micros(50));
        }
        assert!(
            bridge.runtime().is_idle(),
            "replacements never became ready"
        );

        fake.observed_peak = GpuPeakObservations::default();
        bridge.totals.peak_chunk_mesh_committed_bytes = 0;
        bridge.totals.peak_staged_bytes = 0;
        bridge.totals.peak_chunk_mesh_total_bytes = 0;
        let report = match bridge.update(&mut fake) {
            Ok(report) => report,
            Err(error) => panic!("update failed: {error}"),
        };
        assert!(report.uploads > 0 && report.commits > 0);
        let end_sample = bridge.chunk_mesh_gpu_bytes();
        assert_eq!(end_sample.committed, fake.residency().bytes());
        assert_eq!(end_sample.staged, fake.residency().staged_bytes());

        let totals = bridge.totals();
        assert_eq!(
            totals.peak_chunk_mesh_total_bytes, fake.observed_peak.total,
            "bridge peak must equal the presentation's mutation-boundary oracle"
        );
        assert_eq!(
            totals.peak_chunk_mesh_committed_bytes,
            fake.observed_peak.committed
        );
        assert_eq!(totals.peak_staged_bytes, fake.observed_peak.staged);
        assert!(
            fake.observed_peak.total > end_sample.total(),
            "this transition must expose the peak that update-end sampling misses"
        );
        assert!(fake.observed_peak.staged > 0);
        assert!(
            totals.peak_chunk_mesh_total_bytes
                <= fake.observed_peak.committed + fake.observed_peak.staged,
            "the real peak can never exceed the sum of the component peaks"
        );
    }

    /// Restaging releases the obsolete replacement before acquiring its
    /// successor, so presentation-owned chunk-mesh bytes never hold two
    /// replacements of one chunk at the same instant. The bridge cannot see
    /// inside the call, so the contract is proven here.
    #[test]
    fn restaging_releases_the_obsolete_replacement_before_acquiring_the_new_one() {
        let small = voxel_mesh(&[(0, 0, 0)]);
        let large = voxel_mesh(&[(0, 0, 0), (4, 0, 0), (8, 0, 0), (12, 0, 0)]);
        let coord = ChunkCoord::new(0, 0, 0);
        let mut fake = FakePresentation::default();

        let small_bytes = match fake.stage_chunk(coord, LodLevel::Lod0, &small) {
            Ok(bytes) => bytes,
            Err(error) => panic!("staging failed: {error}"),
        };
        let large_bytes = match fake.stage_chunk(coord, LodLevel::Lod0, &large) {
            Ok(bytes) => bytes,
            Err(error) => panic!("restaging failed: {error}"),
        };
        assert!(small_bytes > 0 && large_bytes > small_bytes);
        assert_eq!(
            fake.released_before_restage(),
            1,
            "the restage must release the obsolete replacement"
        );

        // Exactly one logical replacement is staged, and it is the new one.
        assert_eq!(fake.staged_bytes_of(coord), Some(large_bytes));
        assert_eq!(fake.residency().staged(), 1);
        assert_eq!(fake.residency().staged_bytes(), large_bytes);
        assert_eq!(
            fake.observed_peak().staged,
            large_bytes,
            "two replacements of one chunk were held at once"
        );
        assert!(fake.observed_peak().staged < small_bytes + large_bytes);

        // An empty restage still replaces the staged mesh and releases it.
        let empty = Mesh::default();
        match fake.stage_chunk(coord, LodLevel::Lod0, &empty) {
            Ok(bytes) => assert_eq!(bytes, 0, "an empty mesh holds no buffers"),
            Err(error) => panic!("empty restage failed: {error}"),
        }
        assert_eq!(fake.staged_bytes_of(coord), Some(0));
        assert_eq!(fake.residency().staged_bytes(), 0);
        assert_eq!(
            fake.observed_peak().staged,
            large_bytes,
            "an empty restage cannot raise the staged peak"
        );
    }

    /// A restage never activates what was staged before and never disturbs the
    /// committed mesh, which keeps drawing throughout.
    #[test]
    fn restaging_never_activates_the_previous_replacement_or_the_committed_mesh() {
        let committed = voxel_mesh(&[(0, 0, 0)]);
        let first = voxel_mesh(&[(0, 0, 0), (4, 0, 0)]);
        let second = voxel_mesh(&[(0, 0, 0), (4, 0, 0), (8, 0, 0)]);
        let coord = ChunkCoord::new(-1, 0, 2);
        let mut fake = FakePresentation::default();

        let committed_bytes = match fake.stage_chunk(coord, LodLevel::Lod0, &committed) {
            Ok(bytes) => bytes,
            Err(error) => panic!("staging failed: {error}"),
        };
        if let Err(error) = fake.commit_staged_group(&[coord]) {
            panic!("commit failed: {error}");
        }
        assert_eq!(fake.active(), vec![coord]);
        assert_eq!(fake.committed_bytes_of(coord), Some(committed_bytes));

        let first_bytes = match fake.stage_chunk(coord, LodLevel::Lod0, &first) {
            Ok(bytes) => bytes,
            Err(error) => panic!("staging failed: {error}"),
        };
        let second_bytes = match fake.stage_chunk(coord, LodLevel::Lod0, &second) {
            Ok(bytes) => bytes,
            Err(error) => panic!("restaging failed: {error}"),
        };
        assert!(first_bytes != second_bytes);
        assert_eq!(fake.released_before_restage(), 1);

        // The drawn mesh is still the committed one, never a staged one.
        assert_eq!(fake.active(), vec![coord]);
        assert_eq!(fake.committed_bytes_of(coord), Some(committed_bytes));
        assert_eq!(fake.residency().bytes(), committed_bytes);
        assert_eq!(fake.staged_bytes_of(coord), Some(second_bytes));
        assert_eq!(fake.residency().staged(), 1);
        assert_eq!(
            fake.observed_peak().total,
            committed_bytes + second_bytes.max(first_bytes),
            "the peak must never include both replacements"
        );

        // Committing swaps in the newest replacement, not the released one.
        if let Err(error) = fake.commit_staged_group(&[coord]) {
            panic!("commit failed: {error}");
        }
        assert_eq!(fake.committed_bytes_of(coord), Some(second_bytes));
        assert_eq!(fake.residency().staged(), 0);
    }

    /// A stage rejected before any allocation must leave valid staging alone.
    #[test]
    fn a_stage_rejected_before_allocation_keeps_the_previous_staging() {
        let staged = voxel_mesh(&[(0, 0, 0), (4, 0, 0)]);
        let rejected = voxel_mesh(&[(0, 0, 0), (4, 0, 0), (8, 0, 0)]);
        let coord = ChunkCoord::new(3, 1, -2);
        let mut fake = FakePresentation::default();

        let staged_bytes = match fake.stage_chunk(coord, LodLevel::Lod0, &staged) {
            Ok(bytes) => bytes,
            Err(error) => panic!("staging failed: {error}"),
        };
        fake.reject_next_stage();
        match fake.stage_chunk(coord, LodLevel::Lod0, &rejected) {
            Err(ChunkUploadError::TooManyIndices {
                coord: rejected, ..
            }) => {
                assert_eq!(rejected, coord);
            }
            Ok(_) => panic!("the injected rejection did not fire"),
        }

        assert_eq!(
            fake.staged_bytes_of(coord),
            Some(staged_bytes),
            "a rejected stage must not destroy valid staging"
        );
        assert_eq!(fake.released_before_restage(), 0);
        assert_eq!(fake.residency().staged(), 1);
        // The still-valid staging commits normally afterwards.
        if let Err(error) = fake.commit_staged_group(&[coord]) {
            panic!("commit failed: {error}");
        }
        assert_eq!(fake.committed_bytes_of(coord), Some(staged_bytes));
    }

    /// The presentation answers for a whole group or not at all: one member
    /// without staging refuses the commit and leaves every slot untouched, so
    /// no pair can be drawn mixing an old and a new seam.
    #[test]
    fn a_group_missing_one_staged_member_commits_no_member_at_all() {
        let mesh = single_voxel_mesh();
        let members = [
            ChunkCoord::new(0, 0, 0),
            ChunkCoord::new(1, 0, 0),
            ChunkCoord::new(2, 0, 0),
        ];
        let mut fake = FakePresentation::default();
        for coord in members {
            if let Err(error) = fake.stage_chunk(coord, LodLevel::Lod0, &mesh) {
                panic!("staging failed: {error}");
            }
        }
        assert!(fake.lose_staging(members[1]), "the member was staged");

        let error = match fake.commit_staged_group(&members) {
            Err(error) => error,
            Ok(()) => panic!("a group with a missing staged member must be refused"),
        };
        assert_eq!(error.coord, members[1]);
        assert_eq!(error.group_size, members.len());
        assert!(
            fake.active().is_empty(),
            "a refused group must draw no member"
        );
        assert_eq!(fake.residency().active(), 0);
        for coord in [members[0], members[2]] {
            assert!(
                fake.staged.contains_key(&coord),
                "{coord:?} must keep its staging for a later complete commit"
            );
        }

        if let Err(error) = fake.stage_chunk(members[1], LodLevel::Lod0, &mesh) {
            panic!("restaging failed: {error}");
        }
        assert!(fake.commit_staged_group(&members).is_ok());
        assert_eq!(fake.active().len(), members.len());
    }

    /// The bridge marks nothing committed anywhere when the presentation
    /// cannot swap a whole group: the runtime keeps its committed meshes and
    /// the refused replacement never becomes the drawn one.
    #[test]
    fn a_refused_group_commit_leaves_runtime_and_bridge_uncommitted() {
        let mut fake = FakePresentation::default();
        let mut bridge = banded_bridge_at(Vec3::new(5.0, 5.0, 5.0));
        settle(&mut bridge, &mut fake);
        assert!(matches!(
            bridge.track_camera(Vec3::new(37.0, 5.0, 5.0)),
            CameraAnchor::Moved(_)
        ));

        let mut sabotaged: BTreeMap<ChunkCoord, MeshStamp> = BTreeMap::new();
        let mut refused = false;
        for _ in 0..20_000 {
            // Take the GPU state away from everything the bridge believes is
            // staged and visibly non-empty, so the next refused group also
            // exercises the ready-visible transition-blocked accounting.
            for coord in bridge.staged_coords() {
                let Some(stamp) = bridge.staged_stamp(coord).copied() else {
                    continue;
                };
                let visible = bridge
                    .runtime()
                    .ready_mesh(coord)
                    .is_some_and(|(_, mesh)| !mesh.indices().is_empty());
                if visible && fake.lose_staging(coord) {
                    sabotaged.insert(coord, stamp);
                }
            }
            if let Err(error) = bridge.update(&mut fake) {
                panic!("update failed: {error}");
            }
            for (coord, stamp) in &sabotaged {
                assert_ne!(
                    bridge.presented_stamp(*coord),
                    Some(stamp),
                    "{coord:?} drew a replacement the presentation refused"
                );
                assert_ne!(
                    bridge.runtime().committed_mesh(*coord).map(|(s, _)| s),
                    Some(stamp),
                    "{coord:?} was committed although the presentation refused"
                );
            }
            if bridge.totals().presentation_commit_failures > 0 {
                assert_eq!(
                    bridge.gaps().ready_undrawn_now,
                    0,
                    "a refused replacement is not a hole while its old committed mesh remains drawn"
                );
                assert_eq!(bridge.gaps().ready_awaiting_upload_now, 0);
                refused = true;
                break;
            }
            thread::sleep(Duration::from_micros(50));
        }
        assert!(refused, "the presentation never refused a group commit");
        assert_eq!(
            bridge.totals().commit_invariant_failures,
            0,
            "the runtime must never commit fewer chunks than the presentation swapped"
        );
        assert_presented_seams_coherent(&bridge, &fake);
    }

    #[test]
    fn committed_missing_is_an_invariant_failure_not_budget_latency() {
        let mut fake = FakePresentation::default();
        let mut bridge = banded_bridge_at(Vec3::new(5.0, 5.0, 5.0));
        settle(&mut bridge, &mut fake);
        let coord = bridge
            .runtime()
            .render_committed_meshes()
            .find(|(_, _, mesh)| !mesh.indices().is_empty())
            .map(|(coord, _, _)| coord)
            .unwrap_or_else(|| panic!("diagnostic source produced no visible committed mesh"));
        assert!(bridge.presented.remove(&coord).is_some());

        bridge.account_gaps();
        assert_eq!(bridge.gaps().committed_missing_now, 1);
        assert_eq!(bridge.gaps().ready_undrawn_now, 0);
    }

    #[test]
    fn content_change_still_invalidates_immediately() {
        let mut fake = FakePresentation::default();
        let mut bridge = banded_bridge_at(Vec3::new(5.0, 5.0, 5.0));
        settle(&mut bridge, &mut fake);
        let origin = ChunkCoord::new(0, 0, 0);
        assert!(fake.active().contains(&origin));
        let replaced = match bridge.runtime_mut().replace_content(origin, Chunk::empty()) {
            Ok(replaced) => replaced,
            Err(error) => panic!("replace failed: {error}"),
        };
        assert!(replaced);
        let report = match bridge.update(&mut fake) {
            Ok(report) => report,
            Err(error) => panic!("update failed: {error}"),
        };
        assert!(
            report.deactivations >= 1,
            "content change must deactivate now"
        );
        assert!(
            !fake.active().contains(&origin),
            "stale content must not stay drawn"
        );
        assert!(bridge.presented_stamp(origin).is_none());
        settle(&mut bridge, &mut fake);
        assert_presented_seams_coherent(&bridge, &fake);
    }

    #[test]
    fn unload_still_invalidates_immediately() {
        let mut fake = FakePresentation::default();
        let mut bridge = banded_bridge_at(Vec3::new(5.0, 5.0, 5.0));
        settle(&mut bridge, &mut fake);
        assert!(fake.residency().active() > 0);
        assert!(matches!(
            bridge.track_camera(Vec3::new(900.0, 5.0, 900.0)),
            CameraAnchor::Moved(_)
        ));
        let report = match bridge.update(&mut fake) {
            Ok(report) => report,
            Err(error) => panic!("update failed: {error}"),
        };
        assert!(report.demand_changed);
        assert_eq!(
            fake.residency().active(),
            0,
            "unloaded chunks must stop drawing now"
        );
        assert_eq!(bridge.presented_count(), 0);
        assert_eq!(bridge.staged_count(), 0);
    }

    #[test]
    fn baseline_profile_never_sends_lod1() {
        let mut fake = FakePresentation::default();
        let mut bridge = match StreamingBridge::new(
            StreamingConfig::m3c_baseline(),
            UploadBudget::default(),
            Vec3::new(5.0, 5.0, 5.0),
        ) {
            Ok(bridge) => bridge,
            Err(error) => panic!("bridge failed to start: {error}"),
        };
        settle(&mut bridge, &mut fake);
        assert!(
            fake.levels_sent
                .iter()
                .all(|(_, lod)| *lod == LodLevel::Lod0)
        );
        assert_eq!(fake.residency().lod1, super::LevelResidency::default());
        assert_eq!(bridge.totals().uploads_lod1, 0);
        assert_eq!(
            bridge.presented_count(),
            bridge.runtime().demand().render.len() - bridge.runtime().summary().render_known_absent
        );
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
        assert_eq!(bridge.staged_count(), 0);
    }
}
