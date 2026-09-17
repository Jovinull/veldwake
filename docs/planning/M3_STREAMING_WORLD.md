# M3 — Streaming World

Status: **M3A and M3B complete and merged; M3C0–M3C3 implemented and independently hardened on the feature branch; LOD remains opt-in pending external review and merge**
Planning branch: `feat/m3c-lod-debug`

## Completed submilestone: M3A — Multi-chunk Correctness

M3A proves coordinate and seam correctness for a small static set of chunks. Despite the parent milestone name, it does not implement loading, residency, scheduling, generation, persistence, or LOD. Those capabilities require later scope backed by M3A evidence.

M3A merged into `main` through [PR #3](https://github.com/Jovinull/veldwake/pull/3) at merge commit `af1cabc913a9500eefbcd647a56c881d2c1288c0`.

## Implementation result

- `veldwake-voxel` now owns `ChunkCoord(i32)`, `WorldVoxelCoord(i64)`, checked Euclidean conversion, and checked axial neighbors. These remain experimental runtime coordinates, not persisted formats.
- `ChunkNeighborhood` is a borrowed, allocation-free center-plus-six-neighbors view. Sampling returns `Known(VoxelId)` or `Missing`; callers must select `BoundaryPolicy::Expose` or `BoundaryPolicy::RequireKnown` explicitly.
- The M2 `mesh_exposed_faces(&Chunk)` entry point remains as an `Expose` compatibility wrapper. Neighbor-aware meshing removes a shared face for any solid ID pair and reports `MeshBoundaryError` when `RequireKnown` needs absent data.
- The canonical fixture is an ordered `Vec` at `(-1, 0, 0)`, `(0, 0, 0)`, `(0, 0, 1)`. It contains 51 solids, has fingerprint `0xe65ae5533c4db16a`, and produces 202 quads, 808 vertices, and 1,212 indices after seam removal.
- The client builds all three meshes once, keeps their vertices chunk-local, and supplies one immutable model uniform/bind group per chunk. Aggregate upload is 24,288 bytes: 19,392 vertex bytes, 4,848 index bytes, and 48 model-uniform bytes.
- The final release probe observed 76 microseconds for the three-chunk mesh on the audited host. This is one diagnostic observation, not a budget or benchmark.

## Scope and acceptance criteria

- A GPU-independent `ChunkCoord` with signed integer components and explicit axis conventions.
- Explicit, checked conversion between `(ChunkCoord, LocalCoord)` and a signed world-voxel coordinate.
- Correct Euclidean behavior at zero and negative chunk boundaries.
- Neighbor-aware exposed-face meshing that removes both faces at a populated seam.
- A local neighborhood API: the mesher must not know world storage, `ChunkCoord`, renderer, or GPU types.
- One deterministic fixture containing at least two adjacent chunks, including a negative-coordinate boundary and more than one `VoxelId`.
- Exact headless seam tests in all six axial directions, plus missing-neighbor boundary tests.
- A small, static set of chunks rendered together with correct relative placement.
- Mesh vertices remain chunk-local; the client applies one transform/offset per chunk rather than baking duplicated world coordinates into CPU meshes.
- Existing M2 single-chunk topology, fingerprint, winding, and headless behavior remain valid.

## Coordinate contract

### Types and axes

- `ChunkCoord { x: i32, y: i32, z: i32 }` identifies a chunk in the existing right-handed, Y-up grid. It is a small value type with equality, hashing, ordering, checked axial-neighbor helpers, and no rendering behavior.
- `WorldVoxelCoord { x: i64, y: i64, z: i64 }` identifies a voxel cell globally. Using `i64` keeps `i32` chunk origins multiplied by `CHUNK_EDGE = 32` representable without overflow and does not imply an infinite or persisted world.
- Existing `LocalCoord` remains bounded to `0..CHUNK_EDGE` on every axis. It does not acquire negative values.
- These are experimental runtime coordinates, not a save/network compatibility promise.

### Conversion

For each axis, converting chunk/local to world uses:

```text
world = i64(chunk) * CHUNK_EDGE + local
```

The reverse conversion uses Euclidean quotient and remainder, never truncation toward zero:

```text
chunk = world.div_euclid(CHUNK_EDGE)
local = world.rem_euclid(CHUNK_EDGE)
```

The quotient must fit `i32`; otherwise conversion returns a typed range error. Canonical boundary examples for edge 32 are:

| World voxel | Chunk | Local |
|---:|---:|---:|
| `32` | `1` | `0` |
| `31` | `0` | `31` |
| `0` | `0` | `0` |
| `-1` | `-1` | `31` |
| `-32` | `-1` | `0` |
| `-33` | `-2` | `31` |

Tests must cover round trips, all axes, exact boundaries, negative values, and range failures. Conversion belongs with voxel spatial primitives, not in the renderer.

## Neighbor sampling alternatives

### Callback or sample function

Pass a closure that receives an out-of-bounds local coordinate and returns a voxel/missing result.

- Strengths: minimal storage assumptions; can adapt maps, procedural sources, or tests.
- Costs: hides which neighbors are required, introduces a call at every boundary sample, complicates lifetimes/generics or dynamic dispatch, and can accidentally let world lookup policy leak into a correctness-reference mesher.
- Decision: not preferred for M3A. It remains a possible later adapter above the local neighborhood boundary.

### Explicit neighborhood view — selected

Pass a borrowed view containing the center chunk plus up to six axial neighbors, addressed by the existing six `Face` directions. Sampling accepts coordinates in the center chunk's local frame and maps only one-cell axial overflow (`-1` or `CHUNK_EDGE`) to the matching neighbor edge.

- Strengths: exactly matches exposed-face meshing needs; dependencies and missing neighbors are visible; no allocation, world map, callback dispatch, or renderer knowledge; easy to construct in tests.
- Costs: specialized to face-adjacent sampling; algorithms needing diagonals or wider stencils will require a different view later.
- Decision: selected for M3A because the specialization is honest and sufficient. Do not generalize for hypothetical generation/lighting algorithms.

The view should preserve a distinction such as `Known(VoxelId)` versus `Missing`; it must not make public chunk reads reinterpret out-of-bounds as AIR. For M3A's finite static set, the meshing call explicitly selects an “expose missing boundary” policy so absent outer neighbors produce faces. A future streaming layer must decide whether unavailable data should defer/remesh work rather than silently treating unloaded chunks as empty.

### Six face slabs or halos

Copy one voxel-thick boundary slabs into a meshing input.

- Strengths: contiguous, self-contained worker input; useful for asynchronous jobs and snapshot/version control.
- Costs: extra copying and ownership/version semantics before jobs or concurrent mutation exist; corners are unnecessary for the current mesher.
- Decision: defer until a later M3 step demonstrates that detached meshing jobs need snapshot halos.

### Whole world/map access

Give the mesher a chunk map or world interface.

- Strengths: straightforward lookup from world coordinates.
- Costs: couples a local geometry algorithm to storage, availability, coordinate, locking, and lifecycle policy; makes headless unit fixtures heavier.
- Decision: rejected for M3A.

## Meshing behavior

- Retain the M2 face-culling algorithm and chunk-local vertex positions.
- Add a neighbor-aware entry point consuming the explicit neighborhood view. The current isolated entry point may remain as a compatibility wrapper with an explicit expose-missing policy.
- For a solid boundary voxel, sample the corresponding neighbor cell. Emit the face for known AIR or an explicitly exposed missing boundary; suppress it for a known solid voxel regardless of differing `VoxelId`.
- Mesh every chunk against the same immutable fixture snapshot. At a shared populated seam, both chunks see the other and independently suppress their opposing faces.
- M3A does not solve invalidation. If a neighbor changes, the affected meshes would need rebuilding; scheduling that rebuild belongs to later M3 work.

## Deterministic fixture and seam tests

The fixture uses a canonically ordered `Vec<(ChunkCoord, Chunk)>`, not a new world/ECS abstraction. It includes:

- chunks at `(-1, 0, 0)`, `(0, 0, 0)`, and `(0, 0, 1)`, exercising negative X and positive Z seams;
- asymmetric cells spanning shared faces with differing voxel IDs;
- table-driven two-chunk unit fixtures covering positive/negative X, Y, and Z exactly;
- exterior cells proving that a missing outer neighbor remains visible under the explicit M3A boundary policy.

For each of the six directions, tests assert exact seam-face removal, not only a visually plausible total. Coverage also includes AIR-versus-solid, solid-versus-solid with different IDs, missing neighbors, coordinate round trips, neighbor-coordinate overflow, and unchanged M2 isolated topology.

## Static rendering implementation

- A small fixed collection is built and meshed only once during client startup.
- Each CPU mesh preserves its `0..CHUNK_EDGE` local positions.
- Renderer-owned immutable buffers are created per diagnostic chunk using the existing presentation adapter.
- `ChunkCoord * CHUNK_EDGE` is applied as one client-owned model uniform/bind group per chunk. World positions are not baked into CPU vertices; instancing/general batching remains out of scope.
- Integer origins become `f32` only at the presentation boundary. The small fixture avoids precision concerns; origin rebasing is a later large-world decision.
- Startup logs chunk count and aggregate solids/quads/vertices/indices/upload bytes once. Existing camera, depth, culling, lifecycle, and error handling are preserved.

## Validation plan

1. Implement coordinate types and exhaustive boundary/round-trip tests without changing rendering.
2. Implement the local neighborhood view and six-direction seam tests headlessly.
3. Add the deterministic adjacent-chunk fixture and record exact topology/fingerprint evidence.
4. Extend the client adapter to render only that small static set using per-chunk translations.
5. Run all repository gates and the release probe; manually inspect seams, placement, negative-coordinate chunk placement, camera traversal, resize/minimize/restore, and clean shutdown on D3D12.

All five steps were exercised on 2026-09-16. Headless coverage includes exact Euclidean boundaries, extrema/range errors, checked neighbor overflow, all six seam directions, different-ID seams, known AIR, both missing-neighbor policies, locked M2 topology, and locked aggregate M3A topology. The Windows smoke used Intel Iris Xe through D3D12 (`Bgra8UnormSrgb`, `Fifo`, observed `Opaque`): negative and positive chunk placement, external faces, seam geometry, multiple colors, camera traversal, repeated resize, minimize/restore, focus reset, and Escape shutdown were visually exercised without observed validation errors.

## Non-goals

- Threads, Rayon, async runtime, job system, queues, prioritization, cancellation, residency, or actual streaming.
- World generation, seeds/biomes, disk cache, saves, compression, or migration.
- LOD, greedy meshing, occlusion/frustum work, render graph, or generalized batching.
- ECS, gameplay, physics, collision, networking, server orchestration, or world simulation.
- Diagonal-neighbor/corner sampling, lighting propagation, or a general-purpose world query API.

## Remaining risks

- Treating missing neighbors as exposed is valid only for the finite M3A boundary. Carrying that behavior into asynchronous streaming would cause transient holes or duplicate seam faces.
- One uniform/bind group per diagnostic chunk is intentionally unscalable; M3A must not present it as the final renderer batching strategy.
- The fixture lookup in the client and probe is linear over three chunks. It is diagnostic assembly code, not a residency/world-storage design.
- Large-world float precision and origin rebasing remain deliberately unresolved; integer coordinates convert to `f32` only for this small presentation fixture.

## Milestone: M3B — Streaming Runtime

Status: **Complete and merged** into `main` through [PR #5](https://github.com/Jovinull/veldwake/pull/5) at merge commit `b5473dbb7836b65e6c6c5662a8abf6f02b6e8043`; the Claude Code entry point followed through [PR #4](https://github.com/Jovinull/veldwake/pull/4) at `994e9863936441606d9bd675e1ea62bc74300bf9`.

### M3B1 implementation result

- `veldwake-streaming` is a real headless crate depending only on `veldwake-voxel` and `std`. It owns deterministic demand sets, CPU residency records, request tokens, priority queues, the bounded worker, and result validation.
- Each coordinate incarnation receives a runtime-global, monotonically increasing, non-zero `RequestToken(u64)`. Load and mesh results carry it; exhaustion is fatal and tested; eviction/re-entry cannot accept an ABA result.
- Residency and mesh states are independent. Only the render set requires a mesh; dependency and retention data can remain CPU-only. Any center or neighbor stamp change immediately removes the current mesh state and queues a replacement only when all six faces are known.
- The worker uses one standard-library thread, a capacity-one job channel, a capacity-one result channel, owned inputs, non-blocking orchestration calls, lazy stale-descriptor validation, and a clean channel-close/join shutdown. Four mesh dispatches force a ready load next.
- Detached meshing copies a 64 KiB center and six `32 × 32` face slabs of 2 KiB each, or marks a source-confirmed absent face `KnownAir`. Maximum logical snapshot payload is 77,824 bytes (76 KiB); snapshot construction is refused while any face is unavailable.
- The finite diagnostic source covers positive and negative coordinates with continuous solid seams and returns only `Present(Chunk)` or `KnownAbsent`. It has no seed, noise, biome, persistence, or disk access.
- The final release `streaming-probe` at center `(0,0,0)` observed 27 render / 81 dependency / 125 retention coordinates, 81 loads, 63 resident chunk payloads (4,128,768 bytes), 18 known absences, 27 ready meshes, 2,064,384 cumulative snapshot bytes, zero stale results on the nominal path, and 5,163 microseconds to idle on the audited host. This is diagnostic timing, not a performance target.
- Headless tests cover demand counts/movement/oscillation, cap-safe teleport, request-token ABA and overflow, stale load/center/neighbor results, known absence versus unavailability, neighbor arrival, unload during work, fairness, slab correctness, borrowed/owned topology equivalence, and worker shutdown.

M3B1 deliberately did not connect demand to the camera and did not upload, retire, or render streaming meshes. M3B2 adds exactly those presentation concerns on top of it.

### M3B1 hardening (applied with M3B2)

- `StreamingConfig::validate` now requires `retention_radius >= render_radius + dependency_halo`, computed with `checked_add`. The dependency set is the render cube grown along each axis by the halo, so a smaller retention radius would retire dependency records in the same update that created them. Typed errors: `RetentionTooSmall`, `RadiusOverflow`, `ZeroResidentCap`, `ZeroEvictionBudget`, `RadiusTooLarge`, `CoordinateOverflow`. Tests cover valid and invalid configurations and prove `render ⊆ dependency ⊆ retention` for every validated configuration.
- CPU eviction finalization is bounded by `max_cpu_evictions_per_update` (default `8`) in deterministic `ChunkCoord` order. A retired record that still owns a `Chunk` keeps counting against the hard cap until released, so a large backlog throttles new loads rather than breaching the cap. `cpu_evictions_finalized` and `eviction_budget_hits` are reported.
- A source-confirmed `KnownAbsent` chunk inside render demand now reports `MeshStatus::NotRequired` instead of `WaitingForNeighbors`; only `LoadQueued`/`Loading` render chunks are waiting for data. The first M3B2 client run exposed nine permanently "waiting" chunks at `y = 2` before this fix.
- `ResidencySummary`, `render_ready_meshes()`, `eviction_backlog()`, and `center()` expose per-frame observability without one accessor per counter.

Finding recorded while testing the backlog: with one worker and finalization running before dispatch, every `poll` frees at least one slot before reserving at most one, so an eviction backlog alone cannot produce a `hard_cap_blocks` event. The cap holds through accounting, not blocking. A deterministic test proves that a full backlog with no release refuses a load, and a real-worker test proves `resident + reserved <= cap` at every poll while a teleport backlog drains to zero.

### M3B2 implementation result

- `veldwake-client` depends on `veldwake-streaming`. The new `streaming` module owns `StreamingBridge`, which wraps the runtime, converts the camera into a demand center, decides what is drawable, and forwards bounded commands to a `ChunkPresentation` implementation. The runtime remains free of `wgpu`, `winit`, `bytemuck`, and client types.
- Camera anchoring uses `floor`, never truncation: each finite axis becomes `i64` through `f64::floor` with an explicit `[-2^63, 2^63)` range check, then `WorldVoxelCoord::split` produces the chunk. Non-finite or out-of-range positions return `CameraAnchorError`, are counted, and keep the previous center; nothing saturates silently. `set_demand_center` runs only when the camera's chunk changes.
- Frame order in `App::redraw`: camera update → `track_camera` → `StreamingBridge::update` (apply pending demand change, one non-blocking `poll`, draw-set reconciliation, budgeted uploads, budgeted releases) → camera uniform → render.
- Drawability and deallocation are separate. Any presented chunk whose `MeshStamp` no longer equals the runtime's current render-demand ready mesh is deactivated in that same update, before any upload and regardless of release budget. Its buffers move to a pending-removal set released at most `max_removals_per_frame` (default `8`) per frame. A stale mesh is never drawn because the unload budget ran out.
- The renderer replaced the static mesh vector with `BTreeMap<ChunkCoord, GpuChunkSlot>` and implements the current staged presentation contract: `stage_chunk`, atomic `commit_staged_group`, `discard_staged`, `deactivate_chunk`, `remove_chunk`, `gpu_payload_bytes`, and `residency`. Only committed drawable entries are drawn. The renderer stores no stamps or generations; the bridge keeps `ChunkCoord -> MeshStamp` for what is presented.
- Upload budget defaults: at most `2` uploads per frame, soft `4 MiB` per frame, one mesh larger than the soft limit may be the frame's only upload and is counted as `oversized_uploads`. Sizes are exact GPU bytes (`24`-byte vertices, `u32` indices, `16`-byte model uniform) computed before upload. A stamp already presented is never re-uploaded. Empty meshes reach no buffers and bypass the budget; presenting one only clears stale buffers.
- Every five seconds the client emits two aggregate lines: camera chunk, demand counts, tracked/CPU-resident/`KnownAbsent`/evict-pending, waiting/dirty/meshing/ready, queues and in-flight jobs, presented/GPU-resident/GPU-active/pending removal; then dispatches, stale drops, fairness, cap blocks, evictions, interval and total uploads/bytes, deactivations, removals, deferrals, oversized events, anchor rejections, and snapshot/resident/CPU-mesh/GPU byte totals. No per-chunk-per-frame logging.
- Final review fixes: a chunk presented with an empty mesh never held buffers, so leaving render demand no longer queues it for a budgeted release (empty air layers do not consume release slots); `StreamingBridge::track_camera` returns a `CameraAnchor` enum instead of a `Result`, so the frame loop has no ignored error value.
- Twenty-three client tests are GPU-independent. An in-memory `ChunkPresentation` double proves: the settled draw set equals the render-demand ready set, dependency/retention chunks are never drawn, identical stamps are not re-uploaded, leaving render demand deactivates every stale mesh in one update while releases drain under budget, re-entry rebuilds with new request tokens, oversized meshes upload alone and defer the rest, camera anchoring at `0`, `31.999`, `32`, `-0.001`, `-1`, `-32`, and `-32.001`, and explicit rejection of `NaN`, `±inf`, and out-of-range positions.

### M3B2 observed evidence

Release client on the audited Intel Iris Xe / D3D12 host (`Bgra8UnormSrgb`, `Fifo`, `Opaque`, 1600×900), default camera in chunk `(1, 1, 1)`, first five-second interval: 291 frames at ≈58 FPS (vsync), demand 27/81/125, tracked 81, 51 CPU-resident, 30 `KnownAbsent`, 18 ready meshes (9 non-empty floor chunks plus 9 empty), 9 render chunks `KnownAbsent` at `y = 2`, `mesh_waiting` 0, 9 GPU uploads totalling 2,214,144 bytes, 9 GPU-active, 0 stale results, 0 cap blocks, 0 deferrals. Resident payload 3,342,336 bytes, CPU mesh 2,509,200 bytes, GPU 2,214,144 bytes, snapshot bytes dispatched 1,382,400. A second run in which input moved the camera to chunk `(2, 1, 2)` recorded 2 demand changes, 14 bounded CPU evictions, 12 immediate deactivations, 12 budgeted removals, 0 stale results, 0 anchor rejections, and 0 cap blocks. The release `streaming-probe` reached idle in 7,660 µs with unchanged counts. These are one-host observations, not targets.

### M3B driven smoke (2026-09-16, release client, Intel Iris Xe / D3D12)

Two scripted runs drove the final build through Win32 input injection (`keybd_event`, `mouse_event` with the right button held), window operations (`MoveWindow`, `ShowWindow`), and `Graphics.CopyFromScreen` captures that were inspected image by image. Verified:

- Traversal crossed positive and negative chunk boundaries on every axis: camera chunks spanned `x = -6..4`, `z = -2..4` in the second run and `y = -1..1` in the first, including leaving the finite corridor and coming back. Screenshots during and after each leg show the 8-voxel checkerboard continuing across chunk borders with no seam line, no wrongly exposed side face, and no hole; two captures one second apart at rest are identical.
- Outside the corridor the client draws nothing (`presented = 0`, `gpu_active = 0`, `known_absent = 85`); no speculative AIR or partial geometry appeared for unloaded chunks.
- Re-entry to the start area rebuilt the original state exactly: 18 presented, 9 GPU-active, 51 CPU-resident, identical checkerboard and landmark.
- Every five-second traversal interval held 60.0 FPS under `Fifo` vsync (16.66–16.67 ms average wall frame) while loads, meshes, uploads, and evictions ran; no stall was measurable at this granularity.
- Mouse look changed pitch and yaw as captured; WASD movement produced the expected demand changes; a key held across a focus loss was released by the input reset and caused no movement after restore.
- Resize to 900×500 and 1500×850 logical (1107×578 and 1857×1016 physical) reconfigured the surface and rendered correctly; minimize suspended the surface at zero size and reset input; restore reconfigured and resumed rendering; Escape logged a clean shutdown and the process exited with code 0.
- Final counters of the second run: 853 loads, 212 meshes, 1 stale load, 0 stale meshes, 0 hard-cap blocks, 319 bounded CPU evictions with 24 budget hits, 106 uploads totalling 26,156,896 GPU bytes, 106 empty meshes, 0 oversized uploads, 0 upload failures, 0 release-budget hits, 0 anchor rejections, 16,179,200 snapshot bytes dispatched; peak resident payload 3,997,696 bytes (61 chunks); peak GPU residency 2,223,504 bytes. `gpu_active <= gpu_resident <= presented <= render` held in every interval.

Not captured: the transient blink when a neighbor arrival invalidates a presented mesh is evidenced by paired `interval_deactivations` and re-uploads in the log, not by a frame-exact capture. This remains one integrated-GPU host.

Accepted visible behavior: when a neighbor arrives or changes, the center mesh is invalidated, its GPU mesh stops drawing immediately, and the chunk reappears after the replacement is meshed and uploaded. The brief gap is deliberate; M3B never keeps a knowingly stale mesh on screen.

M3B should prove bounded movement-driven residency, asynchronous CPU work, stale-result rejection, incremental GPU integration, and safe unload using deterministic diagnostic content. It is not product world generation and does not establish save, networking, gameplay, or long-term content formats.

### Acceptance evidence

- Moving the diagnostic camera across positive and negative chunk boundaries changes a deterministic desired set without blocking the frame callback.
- CPU chunks enter and leave a bounded resident set; meshes are produced off the frame thread and uploaded under explicit per-frame budgets.
- A chunk never displays a mesh computed for an obsolete center or neighbor generation.
- Missing-but-expected neighbors delay meshing instead of becoming AIR. A source-confirmed absent neighbor may expose the corresponding boundary.
- Leaving and re-entering an area safely releases and reconstructs CPU/GPU resources while preserving deterministic fixture fingerprints/topology.
- Tests exercise state transitions, priority ordering, stale load/mesh results, neighbor arrival, unload during in-flight work, negative coordinates, and budget enforcement without requiring a GPU.
- Runtime diagnostics make demand, residency, queues, jobs, stale drops, integration, upload, and unload observable.

### Ownership boundary

The CPU streaming runtime is the sole owner of authoritative resident `Chunk` values in a `BTreeMap<ChunkCoord, ChunkRecord>`. The renderer will own only disposable GPU mesh handles keyed by chunk coordinate and accepted mesh generation. Workers receive owned immutable job inputs and return owned results; they never borrow the resident map or mutate chunks.

For the M3B diagnostic executable, “authoritative” means authoritative for the loaded diagnostic chunk data in that process. It does not move gameplay authority into presentation and does not replace the future local/remote server boundary. The streaming runtime must remain free of `wgpu` and `winit`; the client adapter supplies camera-derived demand and forwards accepted upload/removal commands to the renderer.

M3B1 establishes `veldwake-streaming` as a real CPU/headless boundary. It depends only on `veldwake-voxel` and the standard library. Presentation remains in the client; no empty `world` crate is introduced.

### Demand and residency sets

Convert the camera's finite world position to a `WorldVoxelCoord`, then use the accepted Euclidean conversion to find its center `ChunkCoord`. Recompute demand only when that coordinate or the configured radius changes, not for every sub-voxel camera movement.

The initial diagnostic policy is deliberately small:

- render-demand set: Chebyshev radius 1 on all axes, at most 27 chunks;
- CPU dependency set: render demand plus one axial-neighbor halo, at most 81 chunks;
- retention set: Chebyshev radius 2, at most 125 chunks, to prevent immediate churn near a boundary;
- hard resident limit: 160 chunks during transitions, equal to 10 MiB of raw dense chunk payload before record/container overhead.

Sets are built as `BTreeSet<ChunkCoord>` and priority ties use `ChunkCoord` ordering. Desired CPU residency is the dependency set. The actual resident set may temporarily include retained chunks and in-flight results, but must remain within the hard cap. When the camera crosses a chunk boundary: add newly desired coordinates, retain still-near chunks, mark coordinates outside retention for eviction, invalidate work that is no longer useful, and rebuild deterministic queue priorities.

These radii and limits are diagnostic defaults, not target-world budgets. M3B1 exposes them as configuration values and reports their observed counts.

### Residency state machine

Use one `ChunkRecord` per tracked coordinate with independent residency and mesh state rather than one combinatorial enum.

Residency state:

```text
Untracked
  -> LoadQueued(request_token)
  -> Loading(request_token)
  -> CpuResident(content_generation) | KnownAbsent(request_token)
  -> EvictPending
  -> Untracked
```

- `Untracked` normally means no map entry; a short-lived tombstone is allowed only while an obsolete result can still arrive.
- A runtime-global, monotonically increasing, non-zero `RequestToken(u64)` identifies each coordinate incarnation. Re-entry always receives a new token; tokens are never reused, and exhaustion is a fatal runtime error rather than wrapping.
- Leaving demand before dispatch removes queued work. Leaving demand during a running job invalidates the record/token and marks the record for eviction; any late result remains identifiable as stale.
- `CpuResident` owns the only mutable authoritative `Chunk`. A replacement or future edit increments `content_generation`; zero is reserved as invalid and overflow is a reported fatal invariant violation, never wrapping.
- `KnownAbsent` is bounded source knowledge for a coordinate in the dependency/retention region. It owns no `Chunk` or GPU resource, satisfies neighbor availability as known AIR, and is versioned by the request that produced it.
- `EvictPending` stops new mesh work, requests renderer removal, and becomes untracked after CPU job inputs/results and the presentation handle are no longer current.

Mesh state within a CPU-resident record:

```text
WaitingForNeighbors
  -> Dirty(mesh_generation)
  -> MeshQueued(job_stamp)
  -> Meshing(job_stamp)
  -> CpuMeshReady(job_stamp)
  -> UploadQueued(job_stamp)
  -> RenderResident(job_stamp, render_handle)
```

Neighbor arrival, departure, or content replacement dirties the center and every resident axial neighbor. An old render mesh may remain visible while a replacement is in flight only if its dependencies are still resident and it cannot expose an incorrect seam; otherwise remove it until a valid replacement exists.

Only coordinates in the render-demand set require a mesh. Dependency-halo and retention-only coordinates may remain CPU-resident without mesh or GPU state. Any change to the center content stamp or one of the six neighbor stamps invalidates the current mesh immediately; M3B1 does not retain a knowingly stale mesh while rebuilding.

### Version and job stamps

Each coordinate incarnation receives a runtime-global `RequestToken`; it is not a reusable per-coordinate generation. Each accepted/replaced CPU chunk has a monotonically increasing `content_generation`; each requested mesh has a `mesh_generation`. Both load and mesh jobs/results carry the request token. A mesh job stamp contains:

- coordinate;
- request token and mesh generation;
- center content generation;
- for each of six faces, neighbor coordinate plus content generation, or `KnownAbsent` from the diagnostic source;
- demand epoch for diagnostics, not as the sole correctness check.

A load result is accepted only when its coordinate is still desired/retained and its request token matches. A mesh result is accepted only when the record still exists, request token/mesh generation match, the center generation matches, and all six neighbor stamps still describe current resident data or the same source-confirmed absence. Every other result is counted and discarded without side effects.

### Meshing snapshot alternatives

The dense M3A `Chunk` payload is 64 KiB. One face slab contains `32 × 32 × 2 = 2,048` bytes.

| Alternative | Payload retained/copied per mesh job | Strengths | Costs and risks |
|---|---:|---|---|
| Copy center plus six whole neighbors | Up to 448 KiB | Reuses the existing borrowed neighborhood almost directly; completely self-contained. | Copies seven times more voxel data than the center and retains irrelevant neighbor interiors. Multiple queued jobs amplify memory bandwidth and peak memory. |
| Copy center plus six one-cell slabs/halos | 64 KiB + up to 12 KiB = 76 KiB | Self-contained; exact data required by exposed-face meshing; no cross-thread references or shared mutation; explicit per-face availability. | Requires a small owned snapshot/sampler and deterministic slab extraction tests. Future algorithms needing diagonals/wider stencils need a new snapshot contract. |
| Share immutable chunks with `Arc<Chunk>` | Pointer clones are small; up to 448 KiB of existing chunk payload can remain pinned per job | Avoids voxel copying and overlapping jobs can share allocations. | Makes authoritative edits replacement/COW operations, can retain evicted chunks unexpectedly, and requires careful generation/lifetime accounting. It also exposes full neighbors when only faces are needed. |
| `Arc` center plus copied slabs | About 12 KiB copied plus one pinned 64 KiB center | Reduces copying while keeping neighbor inputs narrow. | Combines two lifetime/ownership models before measurements show center copying matters. |

**Selected for M3B1:** owned center plus six owned slabs/halos. Construct the snapshot only when dispatching, not while queued, so queue entries remain small descriptors. Represent each face as `Cells(2 KiB)` or `KnownAir`; do not dispatch while any required face is `Unavailable`. The job then has at most 76 KiB of voxel payload, is deterministic and `Send` without borrowed `&Chunk`, and uses the same internal geometry path as `RequireKnown` borrowed meshing. This choice does not redefine persisted chunk layout.

### Neighbor availability

Differentiate three conditions before dispatch:

- `Resident`: copy the neighbor's touching slab and generation into the snapshot.
- `KnownAbsent`: the finite diagnostic source states that no chunk exists there; encode a known-AIR face and allow exterior geometry.
- `Unavailable`: the coordinate may exist but is queued/loading/not requested; keep the mesh in `WaitingForNeighbors` and emit no speculative outer face.

The CPU dependency halo should make `Unavailable` temporary for render-demand chunks. A newly arrived neighbor dirties both sides of the seam. M3B must never silently map unloaded data to AIR.

### Minimal worker and queue model

M3B1 uses one dedicated standard-library worker thread and bounded capacity-one job/result channels; do not add Tokio, Rayon, an ECS, or a general job framework. The orchestration thread owns two deterministic priority heaps (`load` and `mesh`) and dispatches at most one owned job at a time. A single worker makes completion order understandable while still proving that source and meshing work leave the orchestration path. Capacity one is sufficient because only one job can be in flight; orchestration uses only non-blocking channel operations.

Priorities are recomputed lazily from the latest camera chunk:

1. mesh work for render-demand chunks that have complete snapshots;
2. diagnostic source loads in the CPU dependency set;
3. remesh work for retained chunks, only if useful;
4. deterministic tie-break: squared chunk distance, then `ChunkCoord`.

To prevent source starvation, after four consecutive mesh dispatches while load work is ready, dispatch the nearest valid load next. Record this fairness intervention as a metric rather than adding a general scheduler.

Queue nodes carry coordinate, kind, and generations only. Before dispatch, validate the node against current state and drop obsolete entries. Do not try to remove arbitrary entries from the heap. One worker means an already running small job is not interrupted: cancellation is cooperative at queue boundaries, while obsolete completed work is rejected by stamps. Add a cancellation token only if later measured jobs become long enough that discarding their completed result wastes material frame time or memory.

### Frame integration and upload budgets

Initial conservative diagnostic limits:

- drain at most 4 completed worker results per frame;
- spend at most 1 ms of main-thread result validation/state integration per frame, stopping when either limit is reached;
- create/upload at most 2 chunk meshes per frame;
- soft upload-byte budget of 4 MiB per frame;
- if one mesh alone exceeds 4 MiB, upload at most that one mesh in the frame and emit an over-budget metric rather than starving it forever;
- retire at most 8 chunk presentation handles and 8 CPU records per frame;
- keep at most one worker job in flight and one completed result buffered in M3B1. Revisit buffering only when M3B2 measures frame integration.

Measure actual integration and upload times separately. These are starting safety rails, not performance targets; changing them requires captured evidence. No disk/network work or snapshot construction occurs in the render callback. Snapshot construction and queue scheduling belong to the orchestration update before rendering and are themselves measured/bounded.

### Safe unload

When a coordinate leaves retention, retire its request token, remove pending descriptors lazily, stop accepting results, and request removal of its renderer handle. The renderer removes the handle at a frame boundary before encoding subsequent draws. Dropping `wgpu` handles is allowed only after no current render list references them; `wgpu` retains underlying resources as required for submitted GPU work. CPU chunk storage may then be dropped unless an already-dispatched owned snapshot contains copies, which are independent and will be discarded on return.

If the coordinate becomes desired again before eviction completes, issue a new request token. Never revive an old result or render handle by coordinate alone.

Before dispatching a load that may return `Present`, reserve one hard-cap slot. At every transition, `resident payloads + payload-bearing EvictPending records + reserved load slots <= hard resident cap`. A `KnownAbsent` result releases its reservation; a stale `Present` result is discarded and also releases its reservation. Loads do not dispatch when no slot can first be reserved.

### Deterministic diagnostic source

Use a finite `DiagnosticChunkSource`, not product world generation. It exposes a bounded coordinate corridor large enough for camera traversal and returns one of `Present(Chunk)` or `KnownAbsent`. Present chunks are constructed from a small code-defined catalog/pattern keyed solely by `ChunkCoord`, including continuous seam features, asymmetric markers, AIR chunks, and positive/negative coordinates. It has no seed, biome, noise, persistence, disk I/O, or claim of terrain generation.

State-machine tests inject typed results directly to exercise stale/out-of-order completion deterministically without adding timing controls to the production source. Scripted demand paths cover positive/negative movement, boundary oscillation, teleport, eviction, and re-entry.

### Observability

Expose through structured diagnostics and a lightweight debug summary:

- current camera chunk and demand epoch;
- render-demand, dependency, retained, CPU-resident, GPU-resident, and evict-pending counts;
- queued/running/completed load and mesh jobs;
- oldest queue age and priority/distance of the next item;
- accepted and stale-dropped results by reason;
- chunks waiting for which unavailable neighbor faces;
- content/mesh generation for a queried coordinate;
- results integrated, CPU integration time, meshes uploaded, upload bytes, and unloads per frame;
- resident raw chunk bytes, snapshot bytes in flight, CPU mesh bytes, and estimated GPU bytes;
- hard-cap/budget hits and oversized-upload events.

Normal logs report transitions in aggregates, not one line per chunk per frame. A coordinate inspection command/view should explain why a chunk is absent, waiting, queued, resident, stale, or evicting.

### M3B non-goals

- Product world generation, seeds, terrain, biomes, structures, or procedural history.
- Saves, disk cache, compression, migration, or network transport.
- LOD, greedy meshing, generalized batching, render graph, GPU-driven rendering, or occlusion work.
- ECS, gameplay, physics/collision, simulation authority, entities, or multiplayer.
- Multiple worker scaling, work stealing, a reusable job system, Tokio, or Rayon without profiling evidence.
- Final origin-rebasing policy or permanent residency/save compatibility contracts.

### M3B risks and decision gates

- Boundary correctness depends on treating `Unavailable` differently from `KnownAbsent`; tests must fail if either becomes implicit AIR.
- Neighbor changes fan out to seven dirty meshes (the chunk plus six axial neighbors). Coalesce by mesh generation before dispatch to avoid queue amplification.
- Camera oscillation can churn demand. Retention radius and lazy invalidation must be tested with a boundary-crossing script.
- Dense 64 KiB chunks are acceptable for this diagnostic radius, but the hard cap and memory telemetry are required before expanding it.
- A single worker may be insufficient, but adding workers before measuring queue latency would add nondeterministic completion and synchronization complexity.
- Snapshot construction copies up to 76 KiB per dispatched mesh. Measure copied bytes and time before considering `Arc`/COW ownership.
- Per-chunk GPU resources remain a diagnostic path. M3B validates lifetime and budgets, not scalable draw submission.
- The `ChunkPresentation` trait exists for one renderer and one in-memory test double. Do not grow it into a render abstraction; add a method only when the bridge needs it.
- Camera positions are `f32`; beyond ±2^24 world units integer precision degrades before any range check trips. Origin rebasing remains a later decision.
- With one worker the bridge sees at most one newly ready mesh per frame, so upload deferrals only occur after a backlog (teleport, re-entry). Measure before assuming the budget binds in steady traversal.
- No ADR is created during this planning change. Create one only if implementation accepts a durable crate/ownership/concurrency contract whose alternatives should be preserved beyond this milestone document.

## Milestone: M3C — Initial LOD + Streaming Debug Visualization

Status: **M3C0–M3C3 implemented and QA-hardened; the measured decision keeps `Lod0Only` as the default profile; F1/F2 debug views are implemented and off by default**.
Planning branch: `feat/m3c-lod-debug`

M3C answers one question with evidence: does a second, coarser level of chunk detail buy visible distance for less CPU/GPU/upload cost than simply widening the full-resolution render radius? It must do so without drawing a knowingly incorrect seam, without ever treating an unloaded chunk as AIR, and while keeping every M3B contract (tokens, generations, stamps, budgets, negative coordinates) intact. It also adds the debug visualization needed to see what the streaming runtime is doing.

### M3C0 — edge-generic dense grid and mesher (implemented)

`Chunk` was fixed at `CHUNK_EDGE = 32` and the mesher iterated that edge. A coarse level needs a dense grid of a different edge meshed by the same exposed-face loop, so M3C0 generalizes the primitive on stable Rust without generic const expressions:

- `DenseGrid<const EDGE: usize>` owns `EDGE³` `VoxelId` cells in a heap slice (`vec![AIR; EDGE * EDGE * EDGE]`), so no `[T; EDGE * EDGE * EDGE]` type is needed. It exposes `VOLUME`, `BYTES`, `empty`, `read`, `write`, `read_local`, `write_local`, `solid_count`.
- `GridCoord<const EDGE: usize>` is the validated local coordinate with `usize` components (no truncating casts; `GridCoord::<300>::new(299, 0, 0)` is locked by test); `LocalCoord = GridCoord<32>`. `ChunkBoundsError` gained a public `edge` field so its message names the violated grid.
- `Chunk = DenseGrid<32>` and `CoarseGrid = DenseGrid<16>` (`COARSE_EDGE = 16`) are type aliases, so every M2–M3B call site (`Chunk::empty()`, `LocalCoord::new`, `CHUNK_BYTES`, fixtures, streaming, client) compiles unchanged.
- `ChunkNeighborhood<'a, const EDGE = CHUNK_EDGE>`, `FaceSlab<const EDGE = CHUNK_EDGE>`, `OwnedMeshingSnapshot<const EDGE = CHUNK_EDGE>`, and `MeshBoundaryError<const EDGE = CHUNK_EDGE>` use const-parameter defaults, so existing names without a parameter still mean the 32-edge types. `mesh_exposed_faces`, `mesh_exposed_faces_with_neighbors`, and `mesh_exposed_faces_from_snapshot` are generic over the edge and share one `mesh_with_sampler` loop; positions are cell units of the meshed grid, so a coarse mesh is scaled at presentation time.
- The only conversion is explicit and deterministic: `Chunk::downsample_2x(&self) -> CoarseGrid`. A coarse cell is solid if any of its 2×2×2 voxels is solid; its material is the most frequent solid `VoxelId`, ties resolved by the lowest ID. It reads local content only, so chunk coordinates (including negative ones) never influence the result. There is no runtime `factor` and no generic `downsample`.

Locked 32-edge evidence is unchanged after the refactor: diagnostic fingerprint `0xa465ff82790404b9` with 132/528/792, multichunk fingerprint `0xe65ae5533c4db16a` with 202/808/1,212, winding, six-direction seams, both boundary policies, and borrowed/owned snapshot equivalence; the release `streaming-probe` still reports 27/81/125, 63 residents, 27 meshes, and 2,064,384 snapshot bytes. New evidence: the coarse grid of the diagnostic fixture has 16 solids and meshes to 82 quads / 328 vertices / 492 indices (8,192 grid bytes, 11,152 mesh bytes), locked by test and printed by `voxel-probe`; a coarse snapshot is 8,192 bytes plus 512 per known slab.

M3C0 tests (ten new): coarse constants, 16-edge bounds/read/write/strides, empty/full downsample, one-voxel floor and isolated voxel surviving, majority material with lowest-ID tie, determinism on the negative-coordinate fixture chunk with exhaustive occupancy comparison, 16-edge meshing (single voxel, full grid), coarse slabs/seams in all six directions with borrowed/owned equality, and a missing coarse neighbor rejected rather than treated as AIR, and the locked coarse diagnostic topology. No streaming, renderer, or client code changed.

### Levels and representation

Two levels only:

| Level | Grid | Cell size | Source | Mesh |
|---|---|---|---|---|
| `Lod0` | `DenseGrid<32>` (today's `Chunk`) | 1 voxel | authoritative resident chunk | existing exposed-face mesh, positions `0..32` |
| `Lod1` | `DenseGrid<16>` | 2 voxels | 2× power-of-two downsample of the resident chunk, computed inside the mesh job from the owned snapshot | same mesher; positions `0..16` scaled by 2 in the model uniform |

A third level is not planned until `Lod1` has evidence. The authoritative CPU chunk is always full resolution; `Lod1` data is a derived, disposable job input and never becomes resident state. Residency, tokens, and content generations are therefore LOD-independent.

Downsample rules (deterministic, order-independent):

- occupancy: a coarse cell is solid if **any** of its 2×2×2 fine cells is solid (occupancy-conservative). Majority would delete the one-voxel-thick diagnostic floor and every thin feature; conservative bulging is the accepted bias and is visible at transitions;
- material: the most frequent solid `VoxelId` among the fine cells, ties broken by the lowest ID;
- local coordinates only; chunk coordinates are untouched, so negative chunks and the origin behave exactly as at `Lod0`. Tests use the M3A negative-boundary chunks.

### Alternatives compared

| Alternative | Cost/benefit | Seams | Decision |
|---|---|---|---|
| Power-of-two voxel downsampling (`Lod1` = 16³) | Quads and GPU bytes fall roughly 4× per chunk; derivation is a 64 KiB read per job, no new resident data; reuses the mesher after M3C0. | Well-defined with the coarse-occupancy seam rule below; testable exhaustively. | **Selected.** |
| Mesh simplification of the `Lod0` mesh (quad merging / greedy) | Reduces vertices but not resident memory or meshing input; merges across material boundaries or loses IDs; greedy meshing is separately excluded from M3. | Cracks appear wherever merged quads meet unmerged neighbors; no local rule guarantees closure. | Rejected for M3C. |
| Same resolution, larger `render_radius` (no LOD) | Zero new code; cost grows with the cube of the radius (27 → 125 → 343 chunks). | None. | **Required as the measured baseline.** M3C reports this configuration next to the LOD one; if it wins on the audited host at the target distance, LOD stays disabled by default. |
| Distant slab / heightfield impostor for the floor | Cheapest possible far geometry. | Only valid for this diagnostic corridor. | Rejected: it would encode the diagnostic source's shape into the renderer. |

### Selection by distance and hysteresis

Selection uses Chebyshev chunk distance `d` from the camera chunk, recomputed only when the camera chunk changes (as M3B already does). The visible set is the cube of radius `visible_radius`; inside it a spatial transition band decides the level:

```text
d <= 1                    Lod0 mandatory
d == 2                    transition band: keep the chunk's previous level
3 <= d <= visible_radius  Lod1 mandatory
visible_radius  = 3       (Lod1 radius; the render set is the radius-3 cube)
dependency_halo = 1
retention_radius >= visible_radius + dependency_halo   (validated; 4)
```

A chunk that enters the visible set through its edge (`d == 3`) starts at `Lod1`; a chunk that first appears inside the band with no previous level (teleport) also starts at `Lod1`, and only `d <= 1` forces `Lod0`. Because `d` changes only on chunk crossings and the band is one chunk wide, a camera oscillating across one boundary never swaps a level; a boundary-oscillation test asserts zero swaps.

Deterministic counts for these defaults, computed from the set definitions:

| Set | Definition | Chunks | Raw payload at 65,536 B |
|---|---|---:|---:|
| `Lod0` mandatory | `d <= 1` | 27 | 1,769,472 |
| transition band | `d == 2` | 98 | 6,422,528 |
| `Lod1` mandatory | `d == 3` | 218 | 14,286,848 |
| render (visible) | radius-3 cube | 343 | 22,478,848 |
| dependency | render plus one axial halo (`343 + 6 × 49`) | 637 | 41,746,432 |
| retention | radius-4 cube | 729 | 47,775,744 |
| single-step transient | old retention ∪ new dependency (one chunk of travel) | 810 | 53,084,160 |

These four numbers are recorded separately because they answer different questions: 343 is what is presented, 637 is what must be resident for seams, 729 is what is kept to avoid churn, and 810 is the transient union of two retention cubes one chunk apart. The M3C diagnostic profile sets `hard_resident_cap = 810` as transient headroom for this profile (about 50.6 MiB of raw dense payload before record overhead); it is not a mathematically required limit, and the cap must never be raised silently because a set grew. The M3B default profile keeps 160. Teleports are throttled through the cap as in M3B: retired payloads keep counting until the bounded eviction releases them. In the diagnostic corridor most tracked chunks outside `y ∈ [-1, 1]` are `KnownAbsent` and hold no payload (231 resident of 637 tracked when settled), so observed usage is far below the headroom. The eviction budget of 8 per update is re-measured against the larger churn.

The performance baseline is `Lod0` only to the same visible distance (`render_radius = 3`, identical dependency, retention, and cap). Debug visualization stays off during every measurement and its box uploads never share or alter the mesh upload budget.

### M3C1 — headless LOD core (implemented)

`veldwake-streaming` now carries levels end to end without touching the renderer or client:

- `LodLevel { Lod0, Lod1 }` and `NeighborPresentation { ContentOnly, Rendered(LodLevel) }` in `types`; `MeshStamp` gains `lod`, and `NeighborStamp::Resident` gains `presentation`.
- `LodSelection { Lod0Only, Banded }` with `LodSelection::select(distance, previous)`: `Lod0Only` is M3B; `Banded` is `d <= 1` → `Lod0`, `d == 2` → previous level (or `Lod1` with no history), `d >= 3` → `Lod1`, using Chebyshev chunk distance. `StreamingConfig::default()` is unchanged (`Lod0Only`); `StreamingConfig::m3c_diagnostic()` is radius 3 / halo 1 / retention 4 / cap 810 / `Banded`.
- Each record stores its desired level as history while retained; `set_demand_center` re-selects every render-demand record, and a change bumps the mesh generation, dirties the chunk and its six neighbors, and counts `lod_swaps`. Eviction or reincarnation loses the history and takes a fresh deterministic decision. Tokens, content generations, residency, the cap, and loads are untouched by level changes.
- `current_mesh_stamp` includes the level and each resident neighbor's presentation (`Rendered(level)` only for render-demand neighbors; dependency/retention neighbors are `ContentOnly`), so an old-level result or a neighbor level change fails the existing stamp comparison. `stale_lod_results` classifies a rejected result whose level differs from the record's current level; it is derived after the same validation, not a second check.
- Snapshots are built per level: a `Lod0` job is `OwnedMeshingSnapshot<32>` with `FaceSlab::coarse_occupancy_of` toward a `Rendered(Lod1)` neighbor and `from_neighbor` otherwise; a `Lod1` job is `OwnedMeshingSnapshot<16>` with the center's `downsample_2x`, `FaceSlab::downsampled_from` toward content-only or `Lod1` neighbors, and `known_air` toward a `Rendered(Lod0)` neighbor (the coarse side always emits its seam). `KnownAbsent` is known AIR at both levels; unavailable neighbors still block. The worker meshes either variant with the same `mesh_exposed_faces_from_snapshot`; `mesh_with_sampler` was not duplicated.
- `ResidencySummary` reports `lod0_desired`, `lod1_desired`, `lod0_ready`, `lod1_ready`; `RuntimeMetrics` reports `lod_swaps` and `stale_lod_results`.
- Scheduler (hardening): the mesh heap sorts by level rank before distance, and a pure `choose_dispatch(mesh_level, load_ready, consecutive)` encodes the contract — a ready `Lod0` mesh first, then a ready load, then a ready `Lod1` mesh; after four consecutive meshes a ready load goes first (`fairness_load_dispatches` counts only a preempted `Lod0` mesh, since a load beating `Lod1` is the contract, not fairness). No scheduler framework exists.
- `stale_lod_results` (hardening) counts a stale result when the center's level changed **or** when any neighbor's presentation (`ContentOnly`/`Rendered(level)`) differs from the stamp for a neighbor that is still resident. Limitation: a neighbor that is no longer resident cannot be compared, so such a result counts only as `stale_mesh_results`.
- Timings (hardening): `TimingStat { count, total_us, max_us }` with `mean_us()` for `snapshot_build` (orchestration, any level), `lod1_derivation` (center `downsample_2x` plus coarse/occupancy/coverage slab derivation — a **subset** of `snapshot_build`, never added to it), `worker_mesh_lod0`, and `worker_mesh_lod1` (measured in the worker per job and carried in the result). Maxima are recorded separately: about 205 µs for a derivation and about 289 µs for a whole snapshot build in the probe run. No per-job logging; `streaming-probe` prints them per profile.

Mixed-resolution seam evidence (voxel crate): `FaceSlab::<16>::downsampled_from` equals `from_neighbor` on the full downsample for every face; `coarse_occupancy_of` expands the same blocks to 32×32. Both, and `Chunk::downsample_2x`, use one shared `CoarseTally` (any-solid occupancy, majority material, lowest-ID tie), so the coarsening rule has a single implementation. A seam oracle checks all six directions × four occupancy cases with different IDs, plus the negative-coordinate fixture pair `(-1, 0, 0)`/`(0, 0, 0)` where the fine `Lod0` seam between the same chunks is byte-identical to M3A. No location receives faces from both sides; no visible air lacks a face.

Coarse→fine seam (hardening): the coarse side no longer always emits. `FaceSlab::<16>::fine_coverage_of(face, &fine_neighbor)` is an explicit coverage mask (a constructor of its own, not `known_air`): a coarse seam cell is marked covered only when all four fine cells of the neighbor's touching layer are solid, and then the coarse quad is suppressed because the fine geometry closes it; with 0–3 of 4 covered the whole coarse quad stays, partly behind fine solids, so no crack can open. The quad is never subdivided in M3C. Tested per direction for 0/4 … 4/4 coverage (4/4 → no interior face; partial → coarse face present; fine cells never emit toward a solid block) and the general oracle now expects the same rule. This is deliberately conservative: a coarse quad behind fine solids is overdraw, not a visual error.

Headless evidence (`streaming-probe --release`, audited host): the default profile is unchanged (27/81/125, 63 residents, 27 `Lod0` meshes, 2,064,384 snapshot bytes, 0 swaps on oscillation). The M3C profile settles at 637 tracked, 231 resident (15,138,816 bytes), 406 `KnownAbsent`, 27 `Lod0` desired and ready, 316 `Lod1` desired with 120 ready (the other 196 render chunks are source-absent), 637 loads, 147 meshes, 5,323,040 CPU mesh bytes, 3,375,104 snapshot bytes (coverage slabs add 512 bytes per coarse job toward a fine neighbor), 0 stale, 0 cap blocks. One boundary crossing and return swaps 9 chunks the first time and 0 the second; after it 36 `Lod0` / 307 `Lod1` are desired.

Where the `Lod1` cost is (same probe run, release): `snapshot_build` mean 71–76 µs, max 289 µs over 264 jobs; `lod1_derivation` mean 64–66 µs, max 205 µs over 258 derivations (the 120 coarse jobs plus every fine job that derives coarse-occupancy faces); `worker_mesh_lod0` mean 165–181 µs, max 520 µs; `worker_mesh_lod1` mean 49–50 µs, max 267 µs. At most one snapshot is built per `poll`, so the derivation adds at most ~0.2 ms to a frame's orchestration against the 1 ms integration rail. Decision: the derivation stays in the orchestration path (`StreamingRuntime::snapshot`); moving it to the worker would need an owned input of two fine layers per face (a one-cell slab cannot reproduce the coarse rule) and buys nothing measurable at this profile. Re-measure if the profile or the mesher changes.

Runtime tests added: `Lod0Only` never selects `Lod1`; banded startup counts (27/316) and per-coordinate levels; approach 3→2→1 promotes only at `d <= 1`; retreat 1→2→3 keeps `Lod0` through the band; repeated 1↔2 oscillation swaps nothing after the first cycle; teleport assigns by distance only; eviction/re-entry forgets history and issues a new token; an old-level result is rejected with `stale_lod_results` while token and content generation survive; a neighbor's level change rewrites the seam stamp and dirties the center; dependency-only neighbors are `ContentOnly`; the M3C profile reaches idle with meshes at both levels under the cap.

### M3C2 — GPU presentation of both levels and the baseline-versus-LOD measurement (implemented)

- Cleanup: `CoarseTally` is `pub(crate)` in the voxel crate and no longer re-exported; the seam derivations reach it through `crate::chunk::CoarseTally`.
- Presentation contract as first introduced in M3C2 used `upsert_chunk`; the transition hardening below superseded it with staged group commits. The current staging call carries only `(coord, lod, mesh)`, and the renderer still never sees tokens, generations, or the full stamp.
- Model transform: `ModelUniform` is two `vec4` (32 bytes, WGSL-aligned): `translation = ChunkCoord * 32` for both levels, never scaled, and `scale.x = 1` (`Lod0`) or `2` (`Lod1`). The vertex shader computes `world = translation + local_position * scale.x`. `gpu_payload_bytes` counts the 32-byte uniform. A test places positive and negative chunks at both levels and proves both span exactly the same `32³` world volume from the same origin.
- GPU residency: active and staged `GpuMeshBuffers` record level and quads; `GpuResidency { lod0, lod1 }` holds per-level committed and staged counts/bytes. The bridge keeps per-level upload counts and bytes. Immediate deactivation for invalid data, budgeted removal, re-entry, empty meshes, and no re-upload of an identical stamp are re-tested with both levels.
- Profiles: `StreamingConfig::m3c_baseline()` is radius 3 / halo 1 / retention 4 / cap 810 / `Lod0Only`; `m3c_diagnostic()` is the same with `Banded`. `StreamingConfig::default()` is untouched (M3B). The client selects a profile from the `VELDWAKE_PROFILE` environment variable (`default`, `m3c-baseline`, `m3c-banded`) with no CLI dependency; unknown values log a warning and use the default. The five-second reports gained per-level desired/ready/GPU counts, quads, bytes, per-level upload totals, swaps, stale-LOD, the four timing totals/maxima, render-submit mean/max per interval, and a one-time `time_to_idle_ms`.

Benchmark (2026-09-16, release client, Intel Iris Xe / D3D12, `Fifo` vsync, no debug views; identical driven path for both profiles: 12 s settle, pitch down, D 8+8 s, A 16+12 s, W 10 s, S 16 s, D 12 s re-entry, rest; both runs performed 378 CPU evictions and 3,2xx loads, confirming the same traversal):

| Metric | `m3c-baseline` (`Lod0Only`) | `m3c-banded` (`Lod0`/`Lod1`) | Change |
|---|---:|---:|---:|
| presented chunks (peak) | 147 | 147 | 0% |
| GPU-active chunks (peak) | 49 (`Lod0` 49) | 49 (`Lod0` 15, `Lod1` 40 at different peaks) | 0% |
| GPU quads (peak) | 100,836 | 43,374 | −57.0% |
| GPU bytes (peak) | 12,101,888 | 5,206,128 | −57.0% |
| CPU mesh bytes (peak) | 13,713,696 | 5,898,864 | −57.0% |
| uploads (total) | 158 | 494 (`Lod0` 199, `Lod1` 295) | +212.7% |
| upload bytes (total) | 39,053,296 | 67,864,768 | **+73.8%** |
| mesh jobs (total) | 473 | 1,262 | +166.8% |
| `lod_swaps` / `stale_lod` | 0 / 0 | 618 / 2 | — |
| snapshot build total | 81,475 µs (max 1,550) | 163,096 µs (max 713) | +100.2% |
| `Lod1` derivation total | 0 | 128,158 µs (max 705) | — |
| worker mesh total | 192,594 µs (`Lod0`, max 3,400) | 223,487 µs (`Lod0` 155,798, `Lod1` 67,689) | +16.0% |
| CPU mesh time on the path (worker + snapshot build) | 274,069 µs | 386,583 µs | **+41.1%** |
| time to idle coverage from start | 13.49 s | 13.21 s | −2% (load-bound, single worker) |
| observed FPS / avg frame | 60.0 / 16.67 ms | 60.0 / 16.67 ms | vsync-bound |
| render-submit mean / max | ≈14.2–15.1 ms / 37.6 ms | ≈14.7–15.8 ms / 55.0 ms | inconclusive at vsync |
| GPU validation errors | 0 | 0 | — |

Decision by the recorded rule (GPU bytes −50%, upload bytes −50%, CPU mesh time not worse, at the same visible distance): **GPU bytes pass (−57%); upload bytes fail (+74%); CPU mesh time fails (+41%).** LOD stays implemented behind `VELDWAKE_PROFILE=m3c-banded`; the client default remains `StreamingConfig::default()` and the radius-3 comparison profile remains `m3c-baseline`. The cause is visible in the counters: every band crossing re-meshes and re-uploads the swapped chunks and their dirtied neighbors (618 swaps, 1,262 mesh jobs against 473), so a moving camera pays the transition cost far more often than it saves resident bytes. The 57% resident-byte saving is real and will matter for a larger visible radius or a static camera; the swap churn is the problem to solve before LOD can be default (candidates, not decisions: a wider band, swap coalescing per crossing, or level-aware upload budgets), and each must be re-measured on this same path.

Driven smoke of the banded profile on the same run (screenshots at every leg plus resize, minimize/restore, focus loss with a key held, Escape → exit code 0): `Lod1` chunks render at the same world footprint as `Lod0` (checkerboard tiles keep their 8-voxel size at every distance; no half-size chunks), the near/far transition shows no seam line, positive and negative chunks behave alike, re-entry rebuilds the scene, two captures three seconds apart at rest are identical (no persistent hole; the transient gaps seen mid-traversal are the KI-009 blink, now also triggered by level swaps), and no GPU validation error was logged. One capture was contaminated by the Windows Start menu opening over the window; the game kept receiving input (the traversal counters match the baseline) and the capture is excluded from the evidence.

The original “not yet done” M3C3 note is superseded by the implemented M3C3 section below.

### M3C2 follow-up — LOD transitions without holes (implemented)

The M3C2 visual review found the KI-009 blink unacceptable at LOD scale: the floor disappeared in whole bands while the camera crossed the `Lod0`/`Lod1` band. This step removes the holes caused by presentation changes alone, without ever drawing an incorrect seam. Debug views (M3C3) were deliberately not started.

**Diagnosis first.** The bridge gained gap accounting (`GapStats`): a render-demand chunk that was drawn, stopped drawing while still in render demand, and has not returned is a gap, attributed to the runtime's last invalidation cause (`InvalidationCause::{LodChange, NeighborPresentation, Membership, Data}`) and measured in update frames. On the M3C2 path with the M3C2 policy (every invalidation deactivates immediately): `m3c-banded` opened 909 gaps, all attributed to presentation causes and none to data, mean ≈84 frames, longest 232 frames (≈3.9 s at 60 Hz), up to 62 chunks missing at once; `m3c-baseline` opened none. The blink was a pure presentation-transition problem, not a data or availability one.

**Transition model.** A render record now separates its *committed* mesh (what the presentation draws) from its *target* (the stamp the current levels and neighbor presentations demand) and a *pending* replacement (`MeshState::CpuReady`). `MeshStamp::geometry_key()` is the stamp without `mesh_generation`; `differs_only_by_presentation()` holds when two stamps describe the same tokens and content generations and differ only in levels or seam contracts. `NeighborStamp::Resident` records a `SeamContract` (`Same`, `CoarseNeighbor`, `FineNeighbor`) derived from the center's level and the neighbor's presentation instead of the raw presentation, so a neighbor entering render demand at the center's own level never invalidates a seam that would not change. `invalidate_mesh(coord, cause)` keeps the committed mesh only for a presentation-only cause (`LodChange`, `NeighborPresentation`, `Membership` while the chunk stays in render demand) whose target differs only by presentation (`committed_retained`); a `Data` cause (content change, eviction, a neighbor unloaded or reloaded), leaving render demand, or losing resident content drops it at once (`committed_dropped`), and an unavailable neighbor is never turned into AIR. A target equal to the committed geometry (a change reverted before it built) restores `MeshState::Committed` without a job.

**Atomic groups.** `transition_groups()` returns connected sets of render-demand chunks whose committed mesh is not their target. Two adjacent dirty chunks are joined only when a drawn side's seam toward the other changes: its presented level, or the dependency stamp it captured toward that face (`seam_changes`). A drawn chunk that is dirty for another face's sake does not pull its neighbor along, two undrawn entrants constrain nothing, and an undrawn entrant next to a drawn neighbor whose seam toward it stays the same appears on its own. The bridge stages every pending replacement (uploads it without drawing, nearest first, under the unchanged upload budget) and commits a group only when every member is CPU-ready for its current target and staged for that exact stamp. `ChunkPresentation::{stage_chunk, commit_staged_group, discard_staged}` replace `upsert_chunk`; the renderer keeps a `GpuChunkSlot { active, drawable, staged, staged_empty }` per chunk and draws only active drawable slots. No drawn pair ever mixes an old and a new seam: the committed set is always mutually coherent.

**Coalescing.** A newer camera change rewrites targets through the stamps: a pending mesh whose stamp no longer matches is re-dirtied by the runtime, a staged replacement whose chunk no longer has a pending target is discarded (`staged_discarded`), and a staged stamp that no longer matches its target is re-staged when the new mesh arrives (`restaged`). Only the latest target is ever built, and a stale staged buffer is never activated.

**GPU accounting.** Staged replacements are counted per level (`LevelResidency::{staged, staged_bytes}`), the bridge records `peak_staged_bytes`, and the five-second reports carry `staged_now`/`staged_bytes`, `gpu_staged`, `transition_commits`, `transition_chunks`, `restaged`, `staged_discarded`, `committed_retained`, `committed_dropped`, `transition_pending`, the gap counters, and the ready-but-undrawn and blocked-group counters below.

**The first group rule starved the frontier.** The first implementation joined every adjacent dirty pair as soon as one side was drawn. Its benchmark reported zero gaps, fewer uploads (325 / 41,998,400 bytes) and fewer mesh jobs (842) than M3C2, yet the mid-movement captures still showed the floor missing in a diagonal band for about four seconds on the −X leg and a void hugging the camera on re-entry. The gap counter could not see it: it counts only chunks that had been drawn, and these were entrants that had never been drawn. Two counters close that blind spot: `ready_undrawn_*` (render-demand chunks whose replacement is CPU-ready but not drawn: now, max, updates, chunk-frames) and `blocked_groups_now`/`blocked_group_max`/`constrained_undrawn_*` (undrawn members of uncommitted groups that also hold a drawn member). Burst captures every 300 ms assembled into contact sheets per leg made the hole visible and datable: `ready_undrawn_max = 21`, 665 updates with a ready-but-undrawn chunk, 5,885 chunk-frames. The cause was the join rule: frontier entrants were joined, through drawn chunks dirtied by the moving `Lod0` ring, to the ring's own group, and diagonal movement re-dirties that ring every ≈1.35 s (one chunk crossing per axis at 12 voxels/s), before the group can mesh and stage every member; the group never committed and the entrants never appeared. The apparent upload saving was that starvation, not coalescing. With the seam-change join rule the same path reports `ready_undrawn_max = 0` over 7,805 updates and `constrained_undrawn_max = 0`.

**Headless tests.** Six new client tests plus the coherence checker: `banded_profile_presents_both_levels_with_per_level_accounting` (committed counts per level, nothing pending at idle), `level_transitions_in_every_axial_direction_open_no_gap` (`Lod0`→`Lod1` and back in ±x, ±y, ±z with no update in which a chunk that stayed in render demand lost its mesh, and a seam-coherent drawn set in every update), `repeated_swaps_on_the_same_crossing_open_no_gap`, `camera_change_before_commit_coalesces_the_target_and_never_activates_stale_staging`, `content_change_still_invalidates_immediately` (`replace_content` bumps the content generation and drops the drawn mesh at once), `unload_still_invalidates_immediately`, and `continuous_diagonal_movement_never_starves_unconstrained_entrants` (a bounded number of updates per step, never settling: every drawn chunk that stays in render demand keeps drawing, every update is seam-coherent, and an undrawn chunk waits on a drawn neighbor only when that neighbor's seam toward it changes). `assert_presented_seams_coherent` is the definition of "no mixed committed/target seam": toward a presented neighbor the contract must match the neighbor's presented level; toward an undrawn neighbor still in render demand it must match the neighbor's target level unless the drawn chunk's own replacement toward that face is pending (they commit together); toward a neighbor outside render demand the old contract may persist (boundary edge effect).

Re-benchmark (2026-09-16, release client, Intel Iris Xe / D3D12, same driven path, lifecycle captures, no burst captures):

| Metric | M3C2 policy, `m3c-banded` | transition model, `m3c-banded` | `m3c-baseline`, same day |
|---|---:|---:|---:|
| gaps opened (drawn chunk lost while in render demand) | 909 (all presentation, 0 data) | 0 | 0 |
| gap length mean / max (update frames) | ≈84 / 232 | — | — |
| max chunks missing at once | 62 | 0 | 0 |
| ready-but-undrawn: max / updates / chunk-frames | not measured | 0 / 0 / 0 | 0 / 0 / 0 |
| largest transition group | — | 78 chunks | 1 |
| `lod_swaps` / `stale_lod` | 618 / 2 | 648 / 0 | 0 / 0 |
| mesh jobs | 1,262 | 1,388 | 501 |
| uploads (total) | 494 | 548 (`Lod0` 226, `Lod1` 322) | 167 |
| upload bytes (total) | 67,864,768 | 76,214,896 (`Lod0` 55,837,472, `Lod1` 20,377,424) | 41,275,744 |
| transition commits / chunks committed | — | 505 / 1,177 | 501 / 501 |
| committed retained / dropped | — | 1,806 / 400 | 0 / 426 |
| staged discarded / re-staged | — | 211 / 0 | 0 / 0 |
| peak staged (undrawn) GPU bytes | — | 4,650,960 | 253,472 |
| peak GPU bytes (committed only; see the correction below) | 5,206,128 | 4,954,960 | 12,101,888 |
| Historical CPU-time metric (snapshot + derivation + worker; derivation was double-counted) | 386,583 µs | 380,377 µs | 164,698 µs |
| time to idle coverage | 13.21 s | 13.31 s | 13.23 s |
| render-submit max | 55.0 ms | 17.8 ms | 18.0 ms |
| GPU validation errors | 0 | 0 | 0 |

Historical acceptance at this step recorded 0 previously drawn gaps and 0 ready-undrawn updates on that run, no persistent crack at rest, seam-coherent drawn sets, and a clean lifecycle smoke. Independent QA preserves the seam/no-blink result but supersedes the broad “zero holes” interpretation: the current split metric later observed two temporary known-geometry frontier deficits (KI-014).

What the model does not change is the swap churn. Uploads and mesh jobs on the moving path are at the M3C2 level (the intermediate run's lower numbers were starvation), so the recorded decision rule still fails on upload bytes (+85% against the same-day baseline), and LOD stays behind `VELDWAKE_PROFILE=m3c-banded`. The historical CPU row above also pointed in the unfavorable direction, but its formula double-counted `lod1_derivation`, which is a subset of `snapshot_build`; retain it only as historical evidence, not as a canonical total. Current comparisons either report worker time explicitly or use `snapshot_build + worker_mesh_lod0 + worker_mesh_lod1`.

**Correction history.** The `4,954,960` above counts only committed meshes. The first hardening added staged replacements and reported `9,800,128` against a `12,101,888` baseline (−19%), but still sampled only at update end. Independent branch QA then proved that a stage and commit in the same update can peak earlier and raised the correctly instrumented banded high-water to `10,053,696` on the same driven path (−16.9%). The current table below is authoritative; earlier figures remain here only as defect history.

Remaining risks: transition groups are unbounded by design (a connected set of changing seams; 78 chunks observed at the corridor start), and while the camera keeps moving the `Lod0` ring's own switch can be deferred by re-dirtying. That costs stale-but-coherent levels on drawn chunks, never a hole, and converges at rest (KI-011). Source-empty layers still take a mesh job and a group slot each.

Gates on the audited host: `cargo fmt --all -- --check` PASS, `cargo clippy --workspace --all-targets -- -D warnings` PASS, `cargo nextest run --workspace` PASS (120 tests; one `LNK1104` retry under the KI-008 policy), `cargo build --workspace --release` PASS, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` PASS after fixing a pre-existing private intra-doc link to `CoarseTally` left by M3C2, `cargo metadata --locked` PASS, `cargo deny check` PASS, `cargo audit` PASS, `git diff --check` PASS, CRLF and relative-link scans PASS. The Windows/D3D12 smoke is the driven benchmark above (exit code 0, 0 GPU validation errors) with mid-movement and burst captures inspected; it remains one integrated-GPU host (KI-003, KI-006).

### M3C2 hardening — atomic presentation commit and the real GPU high-water mark (implemented)

Two defects found in review of the transition model, both fixed before M3C3.

**1. The atomic commit was not atomic end to end.** The bridge called `StreamingRuntime::commit_group`, then `ChunkPresentation::commit_staged` once per coordinate and ignored every result. The runtime and the bridge therefore recorded a commit the presentation might not have performed: a member whose GPU staging was missing kept drawing its old mesh while the runtime considered the new one committed, which is exactly the mixed-seam state the group rule exists to prevent.

The contract is now all-or-nothing on both sides with a runtime-held two-phase preflight:

- `ChunkPresentation::commit_staged_group(&[ChunkCoord]) -> Result<(), PresentationCommitError>` replaces `commit_staged`. The implementation verifies that *every* member has a staged replacement before touching any slot and returns the first offender without changing anything. The error names the coordinate and the group size; it is a value, never a panic.
- `StreamingRuntime::group_is_ready` rejects empty or duplicate membership as well as any unready member. `commit_group_with` preflights the complete group, holds the runtime's exclusive mutable borrow while it invokes the presentation's atomic swap, then commits the already-validated CPU group. Demand changes, job integration, and generation changes therefore cannot enter between preflight and commit. A presentation refusal propagates without changing the runtime.
- The bridge updates its presented map only after `commit_group_with` succeeds. A refusal counts `presentation_commit_failures`, is logged, and leaves the old meshes drawing. A short commit remains counted as `commit_invariant_failures`, but duplicate membership and intervening runtime mutation now have direct regression coverage rather than relying on “nothing should happen between” as an informal assumption.

Tests: `a_group_missing_one_staged_member_commits_no_member_at_all` drives the presentation double directly (one member's staging is dropped, the group is refused, no member is drawn, the untouched members keep their staging, and the same group commits fully once the member is restaged); `a_refused_group_commit_leaves_runtime_and_bridge_uncommitted` runs the real runtime and worker while stripping staging behind the bridge's back and proves that no refused replacement is ever drawn or committed; `a_group_with_one_unready_member_commits_nothing` proves the runtime side in isolation.

**2. Chunk-mesh GPU bytes need mutation-boundary sampling.** End-of-update sampling was itself insufficient: staging and a group commit can coexist in one update, so the old committed buffers and their staged replacements may peak before commit releases the old set. The bridge now observes presentation-owned chunk-mesh bytes after every successful staging mutation and group commit, plus at update end. It keeps committed, staged, and simultaneous-total peaks. This accounting explicitly excludes depth textures, pipelines, fixed/reused debug resources, and internal driver/wgpu retention; it is not global GPU memory. `the_gpu_byte_peak_is_the_highest_simultaneous_total` prepares all replacements, stages and commits them in one update, and proves against an independent presentation-side mutation oracle that the transient peak exceeds the final sample and is not missed.

Historical re-benchmark (2026-09-16, superseded only for the intra-update peak and `ready_undrawn` interpretation):

| Metric | `m3c-baseline` | `m3c-banded` | Change |
|---|---:|---:|---:|
| peak committed GPU bytes | 12,101,888 | 5,640,752 | −53.4% |
| peak staged (undrawn) GPU bytes | 0 | 4,650,960 | — |
| **peak total GPU bytes (simultaneous)** | **12,101,888** | **9,800,128** | **−19.0%** |
| sum of the two component peaks | 12,101,888 | 10,291,712 | (they never peak in the same frame) |
| uploads / upload bytes | 167 / 41,275,744 | 543 / 74,770,896 | +81.1% bytes |
| mesh jobs | 501 | 1,376 | +174.7% |
| `lod_swaps` / `stale_lod` | 0 / 0 | 654 / 0 | — |
| gaps opened / max simultaneous | 0 / 0 | 0 / 0 | — |
| ready-but-undrawn max / updates / chunk-frames | 0 / 0 / 0 | 5 / 19 / 52 (of 7,318 updates) | — |
| transition commits / chunks | 501 / 501 | 509 / 1,156 | — |
| committed retained / dropped | 0 / 411 | 1,774 / 399 | — |
| staged discarded / re-staged | 0 / 0 | 219 / 0 | — |
| largest transition group | 1 | 78 | — |
| `presentation_commit_failures` / `commit_invariant_failures` | 0 / 0 | 0 / 0 | — |
| CPU mesh time on the path | 171,199 µs | 408,525 µs | +138.6% |
| time to idle coverage | 13.43 s | 13.20 s | −2% |
| observed FPS / avg frame | 60.0 / 16.671 ms | 60.0 / 16.670 ms | vsync-bound |
| render-submit max | 17,875 µs | 17,935 µs | — |
| GPU validation errors | 0 | 0 | — |

The end-of-update `residency()` sample had no measurable frame effect, but it was not sufficient evidence for the high-water claim.

The decision remains unchanged: LOD stays behind `VELDWAKE_PROFILE=m3c-banded`, because the recorded rule requires at least half the GPU bytes and half the upload bytes at no CPU cost.

The historical run reported `ready_undrawn` max 5 / 19 updates / 52 chunk-frames. The old counter did not distinguish upload wait from transition blocking and the text incorrectly treated a visually continuous capture as proof of zero holes. The QA instrumentation now counts only non-empty known geometry and separates `frontier_pipeline_pending`, `ready_awaiting_upload`, `ready_blocked_transition`, and `committed_missing`. Any non-zero `ready_undrawn` is an actual temporary coverage deficit; it may be bounded upload latency rather than seam starvation, but it is not renamed away.

#### Independent branch-QA remeasurement (2026-09-17)

Release client, Intel Iris Xe / D3D12, 1600×900, debug `Off`, the same 12 s settle + D 8+8 + A 16+12 + W 10 + S 16 + D 12 + rest path. Both processes exited cleanly by Escape with no GPU validation error. These are presentation-owned chunk-mesh bytes, not total GPU memory.

| Metric | `m3c-baseline` | `m3c-banded` | Change |
|---|---:|---:|---:|
| peak committed chunk-mesh bytes | 12,101,888 | 5,838,128 | −51.8% |
| peak staged chunk-mesh bytes | 253,472 | 4,470,864 | — |
| **peak simultaneous chunk-mesh bytes** | **12,101,888** | **10,053,696** | **−16.9%** |
| peak simultaneous chunk-mesh bytes, later runs | 12,101,888 (one run) | 9,734,816 to 9,800,128 (five runs) | −19.0% to −19.6% |
| uploads / upload bytes | 167 / 41,275,744 | 546 / 74,961,072 | +81.6% bytes |
| mesh jobs / level swaps | 501 / 0 | 1,394 / 654 | — |
| ready-undrawn max / updates / chunk-frames | 0 / 0 / 0 | 2 / 15 / 21 | actual short-lived coverage deficits |
| ready awaiting upload max | 0 | 0 | — |
| ready blocked by transition max / constrained max | 0 / 0 | 2 / 2 | — |
| committed missing max | 0 | 0 | required invariant |
| largest transition group | 1 | 71 | — |
| presentation / runtime commit failures | 0 / 0 | 0 / 0 | — |
| CPU worker mesh time | 117,779 µs | 198,175 µs | +68.3% (timing is noisy) |
| observed FPS / average frame | 60.0 / 16.67 ms | 60.0 / 16.67 ms | vsync-bound |

The banded run's two ready-undrawn chunks were staged members held by a transition group, not upload-budget wait; `committed_missing_max = 0` proves the bridge never omitted a mesh the runtime had already committed. This demonstrates seam-safe atomicity but also proves that “zero holes” is too broad: the implementation prevents mixed seams and prevents previously drawn presentation-only meshes from blinking, while bounded first-presentation deficits can still occur at the moving frontier (KI-014).

### M3C2 hardening — restage allocation order (implemented)

External review found the last accounting gap in the staging lifecycle. `Renderer::stage_chunk` created the new vertex, index, and model buffers **before** replacing `slot.staged`. When a coordinate was restaged, three things coexisted for the duration of that call: the committed mesh, the obsolete staged replacement, and the new buffers still local to the function. The bridge samples after the call returns, so no counter could ever see that overlap.

**The precondition was checked first, not assumed.** `stage_chunk` has exactly two production call sites, both inside `stage_ready_meshes`, whose candidate list is `render_ready_meshes()` filtered by `self.staged.get(coord).map(|(staged, _)| staged) != Some(*stamp)`. So the method is reached over an existing staging only when the bridge's staged stamp is no longer that chunk's target. Such a replacement can never be committed: `commit_ready_groups` requires `staged == target` for every member of a group. `discard_obsolete_staging` runs earlier in the same update and removes staging for chunks that left render demand or lost their pending target, so a surviving mismatch is only ever a changed target. The obsolete replacement is therefore provably dead, and releasing it before the new allocation is safe.

The order is now: reject, release, acquire, install.

1. An empty mesh clears the staging and marks the commit as a removal, with no allocation at all.
2. `u32::try_from` on the index count rejects before any state is touched, so an early rejection can never destroy valid staging.
3. The obsolete replacement is taken out of the slot and dropped before the new buffers exist.
4. The new replacement is installed into an empty staging position.

`slot.active` and `slot.drawable` are not touched anywhere in this path: the committed mesh keeps drawing across a restage, exactly as before. The ordering is now part of the `ChunkPresentation::stage_chunk` contract and the in-memory double mirrors it, including an observation between release and acquire so the double's independent peak oracle passes through the same instants.

Three regression tests pin the contract: a restage releases before acquiring and the peak staged bytes equal one replacement rather than the sum of two, with an empty restage still replacing correctly and not raising the peak; a restage never activates the previous replacement and leaves the committed mesh drawn, with the later commit swapping in the newest replacement; and an injected rejection before allocation leaves valid staging intact and still committable.

**Measurement: the benchmark path never restages.** Six driven runs report `restaged = 0`: one `m3c-baseline` and three `m3c-banded` at this commit, and two `m3c-banded` rebuilt from the audited `c877130`. The corrected branch is never taken there, so the high-water figures are unchanged by this fix. The reason is structural rather than lucky: a target change re-dirties the record, `ready_mesh` becomes `None`, and `discard_obsolete_staging` releases the staging before the new mesh is ready. A restage needs the replacement to become CPU-ready in the same update that invalidated it, which one worker and a mesh job of hundreds of microseconds make rare. The path is real and reachable, which is why it is covered by unit tests rather than by the benchmark.

**The documented peak is one sample, not a constant.** Re-validating exposed that the banded peak varies between runs of the identical path:

| Binary | `restaged` | peak simultaneous chunk-mesh bytes |
|---|---:|---:|
| audited `c877130` | 0 | 9,800,128 and 9,734,816 |
| with the restage fix | 0 | 9,800,128, three runs |
| QA run recorded below | 0 | 10,053,696 |

Against the stable 12,101,888 baseline that is a saving between 16.9% and 19.6% depending on the run, not a fixed 16.9%. The peak depends on which chunks happen to be staged when the worker and the upload budget line up, and the driven path is time-based. Quote the range, or quote a number with its run; do not treat a single draw as reproducible. The baseline profile is stable at 12,101,888 across every run because its peak is the full resident committed set.

Gates for the restage hardening on the audited host: `cargo fmt --all -- --check` PASS, `cargo clippy --workspace --all-targets --all-features -- -D warnings` PASS, `cargo nextest run --workspace` PASS (144 tests; one `LNK1104` retry under the KI-008 policy), `cargo build --workspace --release` PASS, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` PASS, `cargo test --workspace --doc` PASS (no doc tests), `cargo metadata --locked` PASS, `cargo deny check` PASS, `cargo audit` PASS, `git diff --check` PASS, CRLF and relative-link scans PASS. No separate driven smoke was run: the diff is confined to the staging lifecycle and leaves the presented state of every path identical, and the four driven benchmark runs above exercised the real renderer on D3D12 with exit code 0 and zero GPU validation errors.

### M3C3 — streaming debug visualization (implemented)

No UI framework, no new dependency, no instancing, no render graph. `F1` cycles `Off -> Lod -> Residency -> Boundaries` and `F2` toggles the boxes in the two modes that draw them; both are logged on change and reported in the five-second interval line (`debug_mode`, `debug_boxes`, `debug_draws`, `debug_slots`).

The `debug` module of the client answers one question: given what the runtime and the bridge already know, which primitives should be drawn? It invents no state. Record states come from `StreamingRuntime::tracked_states` (a new query returning each record's residency status, mesh status, level, whether a committed mesh exists, and whether a replacement is pending), presented levels from the bridge's committed stamps, and transition groups from `StreamingRuntime::transition_groups`. The renderer receives placed, colored primitives and draws lines.

- **Lod** tints `Lod1` meshes toward a cool blue in the vertex shader, blending 55% so the diagnostic checkerboard stays readable. The strength travels in the existing camera uniform (`debug.x`) and the level is read from the model uniform's cell size, so no new bind group and no new draw. `Off` sets it to `0.0`, which is a no-op `mix`.
- **Residency** draws one wireframe box per tracked coordinate, colored by record state (`LoadQueued`, `Loading`, `KnownAbsent`, `EvictPending`, `Waiting`, `Dirty`, `Meshing`, `CpuReady`, `Committed`, `NotRequired`) and inset by demand ring (render 0, dependency-only 3, retention-only 6 world units), so the box set equals the tracked set exactly and both facts are visible without a second box per chunk.
- **Boundaries** draws a box on every presented chunk colored by its drawn level, a face outline on the fine side of every mixed-level seam (drawn once, from `Lod0`), an inset box on every chunk whose replacement is meshed (a lighter color once it is also staged on the GPU), and an inset box on every member of an uncommitted transition group, in violet, switching to red at 16 members or more so KI-011 is visible while it happens.

Renderer: one extra `LineList` pipeline sharing the camera bind group, one 72-vertex unit-line buffer (12 cube edges, then six face outlines in `Face::ALL` order), and one 32-byte uniform plus bind group per primitive, pooled and reused across frames. Lines are depth-tested and never write depth. In `Off`, the frame has zero debug primitive allocations, zero debug uniform writes, zero debug draws, and no effect on the mesh upload budget. The fixed pipeline/unit buffer still exist from startup, and slots allocated by an earlier debug mode remain retained for reuse; “off” does not claim zero fixed GPU residency.

**Cost of one bind group per box, measured (2026-09-16, no screen capture, idle camera, banded profile, 12-second windows):**

| View | Boxes drawn | Average frame | Observed FPS |
|---|---:|---:|---:|
| `Off` | 0 | 16.66–16.67 ms | 60.0 |
| `Lod` | 0 | 16.66 ms | 60.0 |
| `Residency` | 637 | 26.6 ms, then 64.8 ms | 37.6, then 15.4 |
| `Boundaries` | 180 | 20.9 ms, 21.4 ms | 47.9, 46.7 |
| `Off` again | 0 | 16.67 ms | 60.0 |

The diagnostic pattern is expensive exactly as the plan warned: hundreds of one-draw, one-bind-group primitives cost tens of milliseconds per frame on the audited host, and `Residency` at 637 boxes drops the client well below 60 FPS. The evidence is recorded (KI-012) and nothing was changed in response: instancing, batching, and a render graph stay out of M3C. `Off` returns to exactly 60 FPS, which is the property the benchmark depends on.

Driven Windows/D3D12 smoke of the views (banded profile, exit code 0, zero GPU validation errors, 11 logged view changes): every mode was entered at rest and during movement; the `Lod` band and its hysteresis are legible while crossing; `F2` visibly clears and restores the boxes in both box modes; `Boundaries` shows mixed seams, pending replacements, and large violet/red transition groups during traversal into negative chunks and back through positives; no mixed seam or disappearance of previously drawn geometry was observed; resize, minimize/restore, focus loss with a key held, and Escape behave as before with a debug view active. This finite capture set did not prove complete frontier coverage; the split counters and KI-014 below supersede any broader visual interpretation. The first residency capture was visually empty because the state colors were too dark against the sky and the floor; they were brightened and re-captured. Residency boxes are depth-tested world-space wireframes, so the diagnostic floor occludes everything below it: residency is read by pitching up or moving, and a non-occluded overlay was deliberately not added.

During the historical smoke `ready_undrawn` reached 13 over 293 updates. The old counter could not attribute those deficits, so the prior explanation (“upload budget”) was a hypothesis, not proof. The current split counters must be used in future smokes. `gaps_closed = 0` proves only that previously drawn chunks did not disappear; it does not prove that every newly ready frontier chunk was visible.

Gates for the hardenings and M3C3 on the audited host: `cargo fmt --all -- --check` PASS, `cargo clippy --workspace --all-targets --all-features -- -D warnings` PASS, `cargo nextest run --workspace` PASS (138 tests: 38 voxel, 50 streaming, 50 client; one `LNK1104` retry under the KI-008 policy), `cargo build --workspace --release` PASS, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` PASS, `cargo test --workspace --doc` PASS (no doc tests), `cargo metadata --locked` PASS, `cargo deny check` PASS, `cargo audit` PASS, `git diff --check` PASS, CRLF and relative-link scans PASS. The Windows/D3D12 evidence is the two benchmark runs and the driven debug smoke above, all exit code 0 with zero GPU validation errors, on one integrated-GPU host (KI-003, KI-006).

Independent branch QA (2026-09-17) reran the full integrated gates without a linker retry: format, Clippy, debug build, release build, 141/141 nextest cases (38 voxel, 50 streaming, 53 client), doc tests, rustdoc with warnings denied, locked metadata, deny, audit, diff/CRLF/link scans, and both release probes all passed. The final Intel Iris Xe/D3D12 smoke covered `default`, `m3c-baseline`, and `m3c-banded`; the targeted banded rerun captured all F1 views, F2 boxes disabled/enabled, movement with debug `Off`, resize, minimize/restore, focus loss, and Escape with empty stderr and no validation error. After visiting debug modes, `Off` retained 670 pooled slots while reporting exactly zero debug draws, primitive allocations, and uniform writes, which is the intended contract rather than zero total debug residency.

### Jobs, stamps, residency, and stale rejection (plan, now implemented as above)

- `MeshStamp` gains `lod: LodLevel` and each `NeighborStamp::Resident` gains the neighbor's **current desired** `lod`. The record stores its desired `lod` next to `mesh_generation`.
- A desired-LOD change bumps `mesh_generation` and dirties the chunk and its six axial neighbors (the seam rule depends on both sides' levels). A result is accepted only if its full stamp, including `lod` and every neighbor `lod`, still matches; a result carrying the old level is counted as `stale_mesh_results` with a new reason counter `stale_lod`.
- Load jobs, request tokens, content generations, the hard cap, reservations, and eviction are unchanged. Only mesh jobs know about levels.
- Queue priority: `Lod0` work for the inner set first, then loads, then `Lod1` work, then retained remeshes; the M3B fairness rule still forces a load after four consecutive meshes.
- `render_ready_meshes()` yields `(coord, stamp, mesh)` with `stamp.lod`; the bridge keys `presented` by coordinate as today, so a LOD swap is just a stamp change.

### Seams between levels

The seam rule is the correctness core of M3C and is decided now, not during implementation:

**Coarse-occupancy seam rule.** At a face shared by chunks of different levels, both sides evaluate the neighbor at the **coarser** resolution.

- Fine side (`Lod0` next to `Lod1`): emit a seam face iff the coarse neighbor block covering that fine cell is AIR.
- Coarse side (`Lod1` next to `Lod0`): emit a seam face iff its own coarse block is solid, regardless of the fine neighbor's contents.
- Same-level seams keep the M3B rule (emit iff the neighbor cell is AIR).

Case analysis for one seam location (fine cell `f`, coarse block `c` covering it): `f` solid / `c` AIR → fine face drawn, coarse none; `f` AIR / `c` solid → coarse face drawn, fine none; both solid → no faces, volumes abut (the coarse bulge is hidden inside); both AIR → nothing. No location ever receives faces from both sides, so no coplanar z-fighting, and no location with visible air on one side lacks a face. Coarse faces behind fine solid cells are occluded by the fine cells' outer faces. Test every case in all six directions with two-chunk fixtures, including negative-coordinate pairs, asserting exact seam face counts and zero duplicated coplanar quads.

Unavailable neighbors still block meshing (`WaitingForNeighbors`); source-confirmed absence is known AIR at both levels; nothing is ever assumed. The `FaceSlab` for a coarse job is the neighbor's full-resolution slab downsampled with the same occupancy rule, so both sides derive the seam from the same data.

### Transitions without incorrect geometry

Planned as the M3B policy (every stale mesh stops drawing in the same update, with `lod_swap_gap_frames` measured per swap) and superseded by the M3C2 follow-up above after measurement: a presentation-only change (level of the chunk or of a neighbor, render-membership churn) keeps the committed mesh drawn until its transition group commits atomically, and a data change, unload, or unavailable neighbor still deactivates immediately. The proof that the old mesh exposes no incorrect seam is the group rule itself: every drawn pair whose seam changes switches in the same update, so the committed set is coherent at every instant. KI-009 is split accordingly.

### CPU/GPU memory and upload implications

- No new resident CPU data: `Lod1` grids live only inside a mesh job (16 KiB grid plus six 16×16 slabs of 512 bytes) and are dropped with the result. Snapshot bytes per `Lod1` job are the same 76 KiB copy plus the derived 19 KiB.
- Expected `Lod1` mesh payload is about one quarter of `Lod0` for the same content (half the linear resolution); the diagnostic floor gives an exact expected quad count to lock in tests.
- The upload budget (2 per frame, 4 MiB soft, oversized alone) is unchanged; uploads carry their level so bytes are reported per level. GPU residency reports `lod0`/`lod1` chunk counts and bytes separately.
- With `visible_radius = 3` the sets are 343 render / 637 dependency / 729 retention chunks and the single-step transient is 810 (50.6 MiB raw); the cap default becomes 810 with that reasoning recorded. The eviction budget of 8 per update is re-measured against the larger churn.

### Negative coordinates and the origin

Downsampling and the seam rule operate on local coordinates (`0..EDGE`), which are non-negative at every level; chunk coordinates, Euclidean conversion, neighbor lookup, and model translation are untouched. The `Lod1` model uniform adds a scale of 2 to the existing signed translation. Tests reuse the M3A fixtures at `(-1, 0, 0)`, `(0, 0, 0)`, `(0, 0, 1)` and the M3B negative corridor.

### Debug visualization

No UI framework. A keyboard toggle (`F1` cycles modes; `F2` toggles boxes) changes a `DebugMode` value owned by the app, logged on change, and carried to the GPU in the existing camera uniform. Modes:

1. `Off` — current rendering.
2. `Lod` — chunk meshes tinted by level (`Lod0` unchanged, `Lod1` cool tint) so transitions and hysteresis are visible.
3. `Residency` — every tracked coordinate drawn as a wireframe box: render/`Lod0` and `Lod1` rings, dependency-only, and retention-only in distinct colors; boxes for `LoadQueued`/`Loading`, `WaitingForNeighbors`, `Dirty`/`Meshing`, `CpuReady`-not-yet-uploaded, `EvictPending`, and `KnownAbsent` use distinct colors or dashed edges.
4. `Boundaries` — wireframe box on every presented chunk only, colored by level, with the seam faces of mixed-level pairs highlighted.

Implementation: one additional `LineList` pipeline with a shared unit-cube edge buffer and one model uniform per box (translation, scale, color), the same diagnostic one-bind-group-per-chunk pattern as meshes; no instancing, batching, render graph, or text rendering. Box data comes from `ResidencySummary`-style queries (`tracked_coords_with_state()`) added to the runtime and from the bridge's presented map. Box counts are bounded by retention size; box uploads are budgeted like mesh uploads. Debug state is testable headlessly (mode cycling, color mapping per state, box set equals tracked set).

Implemented in M3C3 as planned, with these deviations, all recorded above: the runtime query is `tracked_states()`; box uniforms are a reused pool of 32-byte writes that never interact with the mesh upload budget rather than being budgeted like mesh uploads; residency rings are distinguished by inset instead of dashed edges (the `LineList` pipeline has no dash pattern); and `Boundaries` also draws pending replacements and transition groups, which the plan did not anticipate because KI-011 did not exist yet.

### Metrics to decide whether LOD is worth keeping

Reported per five-second interval and captured for the milestone document, for both configurations (`Lod0` only at a wide radius; `Lod0` + `Lod1` at the same visible distance):

- chunks, quads, CPU mesh bytes, GPU bytes, uploads, and upload bytes per level;
- mesh job CPU time per level (measured in the worker) and downsample time;
- LOD swaps, `stale_lod` drops, presentation gaps by cause with mean/max length and peak simultaneous count, ready-but-undrawn chunks, blocked transition groups (implemented in the M3C2 follow-up in place of the planned `lod_swap_gap_frames`);
- time to full coverage after a teleport and after a boundary crossing;
- wall frame time (existing) plus, if it fits in one small step, GPU frame time from `TIMESTAMP_QUERY` (the adapter reports it); otherwise a CPU-side render-submit timing;
- resident payload bytes, snapshot bytes, eviction and cap counters (existing).

The decision rule is recorded before measuring: LOD stays enabled by default only if, at equal visible distance on the audited host, it reduces GPU bytes and per-interval upload bytes by at least half and does not increase mesh CPU time per interval; otherwise it remains available behind configuration for later hardware evidence.

### Headless testing

- M3C0 (done): locked 32-edge fingerprint/topology unchanged; `CoarseGrid` access and bounds; downsample determinism, occupancy-conservative rule, material tie-breaking, empty/full cases, negative-chunk fixture; coarse meshing, slabs, and seams.
- Seams: all six directions × four occupancy cases × two orientations (fine/coarse), exact seam face counts, no coplanar duplicates, source-absent neighbor at each level, unavailable neighbor blocks at each level.
- Selection: ring membership for several radii, hysteresis band, boundary oscillation with zero swaps, teleport producing the expected swap set.
- Stamps: an old-level result is rejected after a swap; a neighbor's level change dirties the seam pair; tokens and generations unchanged by swaps.
- Bridge: presented map records level; per-level byte accounting; swap deactivates before upload; empty `Lod1` meshes bypass the budget; debug box set equals tracked set; mode cycling.
- Probe: `streaming-probe` prints per-level counts for both configurations so the numbers exist without a GPU.

### Acceptance

- `Lod1` renders the diagnostic corridor ring with the coarse-occupancy seam rule and the driven smoke shows no persistent crack or hole at any mixed-level seam, in positive and negative chunks.
- Zero `stale_lod` acceptance; every swap follows mesh → stage → atomic group commit while the old mesh stays drawn, and data invalidations still deactivate first.
- The baseline and LOD configurations are measured and the decision rule is applied and recorded.
- Debug modes make chunk boundaries, levels, demand rings, and record states visible, toggled by keyboard, with no new dependency. Met in M3C3; the boxes are depth-tested, so the diagnostic floor occludes the rings below it.
- All M1–M3B tests pass unchanged apart from names that gain a level parameter.

### M3C non-goals

Product world generation, saves or disk cache, gameplay, physics, ECS, networking, multiple workers, origin rebasing, render graph, generalized batching or instancing, greedy meshing, a third LOD level, text/overlay UI, and any LOD policy beyond one coarse ring.

### M3C risks

- The occupancy-conservative downsample visibly thickens thin features at the ring; acceptable for a diagnostic, but a real terrain will need a material-aware rule and this must be recorded as debt, not hidden.
- Retention at radius 4 (729 chunks) and a cap of 810 multiply resident memory and eviction churn; the eviction budget needs new evidence, not a silent bump.
- Blink of previously drawn chunks on LOD swap was measured in M3C2 (909 presentation gaps, up to 232 frames) and removed by the transition model (0 closed/current gaps on the QA path). New-frontier coverage is a separate metric: QA observed two non-empty ready chunks temporarily withheld by transition groups (KI-014). The churn remains, which is why LOD is not default. Transition groups are unbounded (71 in QA; 78 historically) and continuous movement can defer the ring's own switch (KI-011).
- The seam rule is proven only for axial neighbors at a 2× ratio; a third level or diagonal dependence would need a new proof.
- Debug boxes add up to one draw per tracked coordinate; keep the mode off by default and count the draws.
- M3C0 kept the 32-edge topology byte-identical; the remaining risk is that later LOD work re-introduces edge-specific assumptions instead of using `EDGE`.
