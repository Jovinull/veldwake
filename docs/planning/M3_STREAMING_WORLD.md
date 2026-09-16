# M3 — Streaming World

Status: **M3A implemented locally; pending review/PR and merge**
Branch: `feat/m3-multichunk-foundation`

## Current submilestone: M3A — Multi-chunk Correctness

M3A proves coordinate and seam correctness for a small static set of chunks. Despite the parent milestone name, it does not implement loading, residency, scheduling, generation, persistence, or LOD. Those capabilities require later scope backed by M3A evidence.

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
