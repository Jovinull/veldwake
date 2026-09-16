# M3 — Streaming World

Status: **M3A planning only; implementation not started**  
Branch: `feat/m3-multichunk-foundation`

## Current submilestone: M3A — Multi-chunk Correctness

M3A proves coordinate and seam correctness for a small static set of chunks. Despite the parent milestone name, it does not implement loading, residency, scheduling, generation, persistence, or LOD. Those capabilities require later scope backed by M3A evidence.

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

## Proposed coordinate contract

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

### Explicit neighborhood view — proposed

Pass a borrowed view containing the center chunk plus up to six axial neighbors, addressed by the existing six `Face` directions. Sampling accepts coordinates in the center chunk's local frame and maps only one-cell axial overflow (`-1` or `CHUNK_EDGE`) to the matching neighbor edge.

- Strengths: exactly matches exposed-face meshing needs; dependencies and missing neighbors are visible; no allocation, world map, callback dispatch, or renderer knowledge; easy to construct in tests.
- Costs: specialized to face-adjacent sampling; algorithms needing diagonals or wider stencils will require a different view later.
- Decision: proposed for M3A because the specialization is honest and sufficient. Do not generalize for hypothetical generation/lighting algorithms.

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

## Proposed meshing behavior

- Retain the M2 face-culling algorithm and chunk-local vertex positions.
- Add a neighbor-aware entry point consuming the explicit neighborhood view. The current isolated entry point may remain as a compatibility wrapper with an explicit expose-missing policy.
- For a solid boundary voxel, sample the corresponding neighbor cell. Emit the face for known AIR or an explicitly exposed missing boundary; suppress it for a known solid voxel regardless of differing `VoxelId`.
- Mesh every chunk against the same immutable fixture snapshot. At a shared populated seam, both chunks see the other and independently suppress their opposing faces.
- M3A does not solve invalidation. If a neighbor changes, the affected meshes would need rebuilding; scheduling that rebuild belongs to later M3 work.

## Deterministic fixture and seam tests

The fixture should use a small keyed collection assembled in test/tool code, not a new world/ECS abstraction. Include at least:

- chunks at `(0, 0, 0)` and `(-1, 0, 0)` to exercise the negative X/world boundary;
- asymmetric cells spanning their shared face with at least two voxel IDs;
- additional compact pairs or table-driven rotations covering positive/negative X, Y, and Z;
- exterior cells proving that a missing outer neighbor remains visible under the explicit M3A boundary policy.

For each of the six directions, tests must assert exact seam-face removal, not only a visually plausible total. Also test air-versus-solid, solid-versus-solid with different IDs, missing neighbor, coordinate round trips, neighbor-coordinate overflow, and unchanged M2 isolated topology.

## Static rendering plan

- Build and mesh a small fixed collection only once during client startup.
- Preserve each CPU mesh's `0..CHUNK_EDGE` local positions.
- Create renderer-owned buffers per diagnostic chunk using the existing presentation adapter.
- Apply `ChunkCoord * CHUNK_EDGE` as a per-chunk model translation in client-owned presentation state, preferably one small static uniform/bind group per chunk for this limited proof. Do not bake world positions into every vertex or add instancing/general batching without evidence.
- Convert integer origins to `f32` only at the presentation boundary. The small fixture avoids precision concerns; origin rebasing is a later large-world decision.
- Log chunk count and aggregate solids/quads/vertices/indices/upload bytes once. Preserve existing camera, depth, culling, lifecycle, and error handling.

## Validation plan

1. Implement coordinate types and exhaustive boundary/round-trip tests without changing rendering.
2. Implement the local neighborhood view and six-direction seam tests headlessly.
3. Add the deterministic adjacent-chunk fixture and record exact topology/fingerprint evidence.
4. Extend the client adapter to render only that small static set using per-chunk translations.
5. Run all repository gates and the release probe; manually inspect seams, placement, negative-coordinate chunk placement, camera traversal, resize/minimize/restore, and clean shutdown on D3D12.

## Non-goals

- Threads, Rayon, async runtime, job system, queues, prioritization, cancellation, residency, or actual streaming.
- World generation, seeds/biomes, disk cache, saves, compression, or migration.
- LOD, greedy meshing, occlusion/frustum work, render graph, or generalized batching.
- ECS, gameplay, physics, collision, networking, server orchestration, or world simulation.
- Diagonal-neighbor/corner sampling, lighting propagation, or a general-purpose world query API.

## Open questions and risks

- Confirm during implementation whether checked `ChunkCoord` neighbor overflow should return `Option` or a typed error; do not wrap at `i32` limits.
- Decide the smallest explicit type for `Known` versus `Missing` sampling without building a generic query framework.
- Exact multi-chunk fixture topology/fingerprint must come from implementation evidence, not be invented in planning.
- Treating missing neighbors as exposed is valid only for the finite M3A boundary. Carrying that behavior into asynchronous streaming would cause transient holes or duplicate seam faces.
- One uniform/bind group per diagnostic chunk is intentionally unscalable; M3A must not present it as the final renderer batching strategy.
