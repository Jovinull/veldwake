# M2 — Voxel Prototype

Status: **Active; CPU/headless slice implemented, GPU integration pending**
Branch: `feat/m2-voxel-prototype`

## Purpose

Prove the smallest correct path from GPU-independent voxel data to one visible diagnostic chunk. M2 establishes data, bounds, fixture, meshing, and presentation contracts; it is not a world system.

## Scope

- One fixed-size, CPU-side chunk representation with explicit coordinate and storage conventions.
- One deterministic, code-defined fixture containing enough air/solid boundaries to exercise every face direction and internal-face removal.
- Checked read/write operations and documented out-of-bounds behavior.
- One CPU mesher selected with correctness and basic geometry/timing evidence.
- One immutable mesh result transferred through a narrow presentation adapter and rendered by the existing client.
- Headless correctness tests plus basic CPU/mesh metrics for named fixtures.

## Implemented CPU/headless baseline

These are accepted experimental choices for M2, not save-format or permanent compatibility promises.

### Representation and chunk

- Use a dense heap-backed fixed-size payload of 32³ = 32,768 `VoxelId(u16)` cells: 65,536 logical bytes. The private boxed slice has invariant length; an incrementally maintained solid count makes inspection constant-time.
- Reserve `VoxelId::AIR` (`0`) for air. Material/render metadata remains outside the cell value.
- Linearize as `x + 32 * (y + 32 * z)` in right-handed, Y-up local space. Tests lock corners, strides, uniqueness, and maximum index.
- Public coordinate construction and numeric read/write boundaries are checked. Out-of-bounds produces `ChunkBoundsError` and is never silently interpreted as air; writes return the previous value.
- Keep representation and meshing free of `wgpu`, `winit`, camera, and window types.

### Deterministic fixture

- Define one named fixture entirely in code, without RNG or world generation. The implemented fixture contains 31 solids across IDs 1, 2, and 7: adjacent structures on all six boundaries, a mixed-ID internal face, two isolated voxels, and an asymmetric five-step form.
- Include isolated voxels, adjacent voxels, an enclosed/internal face, all chunk boundaries, and a non-symmetric stepped silhouette so coordinate/winding mistakes are visible.
- Its locked fingerprint is `0xa465ff82790404b9`, computed with documented 64-bit FNV-1a over the edge and every little-endian cell ID. The fingerprint protects fixture construction, not a future save-format promise.

### Mesher selection

- The selected M2 correctness reference is an exposed-face CPU mesher: it emits a quad only when a solid cell borders air or the outside of this isolated fixture chunk. Outside-as-air is not a future seam policy.
- Vertices carry local position, outward normal, face, and `VoxelId`; indices are `u32`. Each quad uses four independent vertices and two counter-clockwise triangles when viewed from outside.
- Measure emitted quads/vertices/indices and wall-clock build time for at least empty, single-voxel, solid, and named-fixture inputs. Exact topology expectations are correctness assertions; timing is diagnostic evidence, not a performance target.
- Face culling is retained for M2 because it is simple, deterministic, and fully testable. Greedy meshing is deferred; no optimization evidence currently justifies adding its material-boundary and winding complexity.

### Headless measurement evidence

Command: `cargo run --release -p veldwake-voxel --bin voxel-probe`. One run on the audited Windows host produced the following diagnostic samples; time is neither a target nor a benchmark threshold. Mesh bytes are logical vertex-plus-index payload and exclude vector capacity/allocator overhead.

| Fixture | Solids | Quads | Vertices | Indices | Chunk bytes | Mesh bytes | Mesh CPU |
|---|---:|---:|---:|---:|---:|---:|---:|
| empty | 0 | 0 | 0 | 0 | 65,536 | 0 | 12 µs |
| single | 1 | 6 | 24 | 36 | 65,536 | 816 | 17 µs |
| solid | 32,768 | 6,144 | 24,576 | 36,864 | 65,536 | 835,584 | 1,086 µs |
| diagnostic | 31 | 132 | 528 | 792 | 65,536 | 17,952 | 34 µs |

The maximum exposed-face checkerboard at edge 32 contains 16,384 solids, 98,304 quads, 393,216 vertices, and 589,824 indices. This proves mathematically that `u16` cannot address the reference mesh; a deliberately huge unit test would add cost without new evidence.

### Rendering integration

- Replace or sit alongside the disposable cube through the smallest explicit adapter from CPU mesh data to renderer-owned GPU buffers.
- Upload only when the single fixture mesh is created or changed; do not regenerate or upload it every frame.
- The renderer consumes mesh output but does not own or mutate canonical chunk cells.
- Preserve the existing camera, depth, lifecycle, diagnostics, and D3D12 smoke path. A render graph, asset system, streaming queue, and generalized resource framework are outside M2.

## Acceptance criteria

- The voxel/chunk and mesher compile and test without a GPU or window.
- Chunk dimensions, coordinate axes, linearization, air semantics, and out-of-bounds behavior are explicit and tested.
- Read-after-write, previous-value return/error behavior, boundary coordinates, and invalid coordinates have meaningful tests.
- The deterministic fixture has documented stable counts/fingerprint and exercises all six face directions plus internal-face culling.
- Empty, single-voxel, adjacent-voxel, solid-chunk, and named-fixture mesh outputs have exact correctness assertions appropriate to the selected mesher.
- The selected baseline mesher is justified by captured topology counts, memory/output size, and basic CPU build timing on named inputs.
- One fixture chunk is visibly rendered with correct winding, depth, camera movement, resize/minimize/restore, and clean shutdown on the audited Windows host.
- Startup or debug diagnostics identify chunk dimensions, solid-cell count, emitted quads/vertices/indices, CPU mesh time, and uploaded byte counts without per-frame spam.
- All repository quality and dependency-security gates pass, and current documentation/handoff reflects actual results.

## Non-goals

- Multiple chunks, chunk coordinates in a world, neighbors, seams, residency, or streaming.
- Procedural terrain/world generation, biomes, seeds, or history.
- LOD, greedy-meshing commitment without evidence, occlusion/frustum systems, indirect rendering, meshlets, or GPU meshing.
- Saves, compression, migration, palette optimization, networking, ECS, gameplay, physics, collision, lighting expansion, textures, or an asset system.
- A generalized job system, render graph, editor, or benchmark suite.

## Evidence and validation plan

1. **Complete:** explicit dense representation, fixture, and CPU mesh contract.
2. **Complete:** headless tests for indexing, bounds, mutation, fixture determinism, six directions, winding, and exact topology.
3. **Complete:** release probe for named fixtures; its timing remains diagnostic only.
4. **Pending:** integrate the immutable CPU mesh through a narrow client-owned upload adapter and inspect the rendered fixture/window lifecycle.
5. **Pending at M2 exit:** final dependency, allocation, error-handling, documentation, and scope review.

## Remaining decision for M2 evidence

- Define only the narrow client-side GPU vertex conversion/upload boundary required to render this CPU mesh. The CPU crate must remain independent of `wgpu`, `winit`, and client types.
