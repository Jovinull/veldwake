# M3 — Streaming World

Status: **M3A complete and merged; M3B (headless runtime plus camera-driven GPU integration) implemented on the feature branch and awaiting review**
Planning branch: `feat/m3b-streaming-runtime`

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

Status: **M3B1 and M3B2 implemented on the feature branch; headless gates pass; the owner's interactive Windows smoke is still pending**.

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
- The renderer replaces the static `Vec<GpuChunkMesh>` with `BTreeMap<ChunkCoord, GpuChunkMesh>` and implements `ChunkPresentation`: `upsert_chunk`, `deactivate_chunk`, `remove_chunk`, `gpu_payload_bytes`, `residency`. Only `active` entries are drawn. The renderer stores no stamps or generations; the bridge keeps `ChunkCoord -> MeshStamp` for what is presented.
- Upload budget defaults: at most `2` uploads per frame, soft `4 MiB` per frame, one mesh larger than the soft limit may be the frame's only upload and is counted as `oversized_uploads`. Sizes are exact GPU bytes (`24`-byte vertices, `u32` indices, `16`-byte model uniform) computed before upload. A stamp already presented is never re-uploaded. Empty meshes reach no buffers and bypass the budget; presenting one only clears stale buffers.
- Every five seconds the client emits two aggregate lines: camera chunk, demand counts, tracked/CPU-resident/`KnownAbsent`/evict-pending, waiting/dirty/meshing/ready, queues and in-flight jobs, presented/GPU-resident/GPU-active/pending removal; then dispatches, stale drops, fairness, cap blocks, evictions, interval and total uploads/bytes, deactivations, removals, deferrals, oversized events, anchor rejections, and snapshot/resident/CPU-mesh/GPU byte totals. No per-chunk-per-frame logging.
- Twenty-three client tests are GPU-independent. An in-memory `ChunkPresentation` double proves: the settled draw set equals the render-demand ready set, dependency/retention chunks are never drawn, identical stamps are not re-uploaded, leaving render demand deactivates every stale mesh in one update while releases drain under budget, re-entry rebuilds with new request tokens, oversized meshes upload alone and defer the rest, camera anchoring at `0`, `31.999`, `32`, `-0.001`, `-1`, `-32`, and `-32.001`, and explicit rejection of `NaN`, `±inf`, and out-of-range positions.

### M3B2 observed evidence

Release client on the audited Intel Iris Xe / D3D12 host (`Bgra8UnormSrgb`, `Fifo`, `Opaque`, 1600×900), default camera in chunk `(1, 1, 1)`, first five-second interval: 291 frames at ≈58 FPS (vsync), demand 27/81/125, tracked 81, 51 CPU-resident, 30 `KnownAbsent`, 18 ready meshes (9 non-empty floor chunks plus 9 empty), 9 render chunks `KnownAbsent` at `y = 2`, `mesh_waiting` 0, 9 GPU uploads totalling 2,214,144 bytes, 9 GPU-active, 0 stale results, 0 cap blocks, 0 deferrals. Resident payload 3,342,336 bytes, CPU mesh 2,509,200 bytes, GPU 2,214,144 bytes, snapshot bytes dispatched 1,382,400. A second run in which input moved the camera to chunk `(2, 1, 2)` recorded 2 demand changes, 14 bounded CPU evictions, 12 immediate deactivations, 12 budgeted removals, 0 stale results, 0 anchor rejections, and 0 cap blocks. The release `streaming-probe` reached idle in 7,660 µs with unchanged counts. These are one-host observations, not targets.

Interactive verification still owed by the owner before merge: continuous traversal across several positive and negative boundaries with chunks appearing and disappearing, no persistent incorrect seam, no speculative AIR for unloaded chunks, camera responsiveness during load/mesh, re-entry reconstruction, a distant teleport staying under the cap, and resize/minimize/restore/focus loss/Escape. Log evidence covers residency, budgets, and stale rejection; it does not replace looking at the screen.

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
