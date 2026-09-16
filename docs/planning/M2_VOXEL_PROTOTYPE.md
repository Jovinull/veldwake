# M2 — Voxel Prototype

Status: **Active planning; implementation not started**
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

## Proposed technical baseline

These are starting hypotheses, not accepted compatibility promises. Implementation evidence may change them within M2, with this document updated in the same work.

### Representation and chunk

- Use a dense fixed-size array as the reference representation. Dense storage makes indexing, deterministic fixtures, mutation semantics, memory cost, and mesher correctness directly observable before compression is justified.
- Start by evaluating a cubic edge of 32 voxels (`32³ = 32,768` cells). Record the chosen edge and byte cost; do not generalize to arbitrary runtime dimensions unless a concrete test requires it.
- Represent cell content with a compact typed identifier, provisionally `VoxelId(u16)`, reserving zero for air. Material/render metadata remains outside the cell value until M2 demonstrates a need.
- Define one canonical linearization order, provisionally `x + EDGE * (y + EDGE * z)`, and test corners, axis strides, uniqueness, and maximum index.
- Accept only checked local coordinates at public read/write boundaries. Reads should distinguish out-of-bounds from air; writes should return the previous value or a typed bounds error rather than silently clamp or ignore input.
- Keep representation and meshing free of `wgpu`, `winit`, camera, and window types.

### Deterministic fixture

- Define one named fixture entirely in code, without RNG or world generation.
- Include isolated voxels, adjacent voxels, an enclosed/internal face, all chunk boundaries, and a non-symmetric stepped silhouette so coordinate/winding mistakes are visible.
- Record stable cell counts and a stable content hash or equivalent canonical fingerprint. The fingerprint protects fixture construction, not a future save-format promise.

### Mesher selection

- Begin with an exposed-face, face-culling CPU mesher as the correctness reference: emit a quad only when a solid cell borders air or the outside of this isolated fixture chunk.
- Make vertex positions, normals, triangle winding, material identifier, and index type explicit. The mesh output is ordinary CPU data and must be testable without a GPU.
- Measure emitted quads/vertices/indices and wall-clock build time for at least empty, single-voxel, solid, and named-fixture inputs. Exact topology expectations are correctness assertions; timing is diagnostic evidence, not a performance target.
- Select the M2 baseline only after this evidence. Greedy meshing may replace the reference only if it preserves tested topology/material boundaries and demonstrates useful geometry reduction on the fixture. Otherwise retain face culling for M2 and defer optimization.

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

1. Record representation alternatives and choose the smallest explicit contract before implementation.
2. Add headless tests for indexing, bounds, mutation, fixture determinism, and topology.
3. Capture mesh counts and debug-profile CPU timing for named fixtures; do not state a target or regression threshold from one host run.
4. Inspect the rendered fixture from multiple angles and exercise the existing window lifecycle.
5. Review dependency direction, allocations, error handling, documentation, and scope before declaring M2 complete.

## Open decisions for M2 evidence

- Confirm or revise the proposed 32-cell edge after memory/index/test ergonomics are measured.
- Confirm whether `VoxelId(u16)` is sufficient for the first material boundary without embedding presentation data.
- Decide whether the exposed-face reference remains the M2 baseline or greedy meshing earns selection from fixture evidence.
- Decide whether the real CPU ownership boundary justifies one focused voxel crate or a smaller existing-package module; do not create empty future crates.
