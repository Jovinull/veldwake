# Testing strategy

Status: **Accepted direction; capabilities arrive with systems**.

## Layers

- Unit tests: local algorithms, invariants, edge cases, and error behavior.
- Integration tests: subsystem boundaries, formats, command flows, caches, and headless authority.
- Property tests: geometry, coordinate transforms, parsers, seed derivation, migrations, and round trips where broad input space matters.
- Golden seeds: stable worldgen and procedural-content fixtures with explicit compatibility intent.
- Visual regression: controlled scene/seed/camera/light/backend captures and perceptual review.
- Fuzzing: save/network/mod/parser boundaries and generator descriptors, initially where platform support is practical.
- Benchmarks/profiling: measured hot paths and representative end-to-end fixtures.
- Soak/replay tests: streaming, save/load, materialization, and long-running simulation when available.

## Test ethics

A failing test is evidence. Do not delete it, reduce its assertion, increase tolerance, regenerate a golden, or add a fallback solely to turn CI green. Explain whether behavior, test, or requirement is wrong; preserve evidence and review intentional changes.

## Deterministic visual fixtures

Planned fixtures include forest, character, sword, creature, and village seeds. Store canonical configuration alongside the baseline: generator/style/renderer versions, camera, light, viewport, quality, and allowed variance. Cross-vendor GPU differences may require backend-specific policy; do not claim pixel identity prematurely.

## Current client applicability

Twenty-three GPU-independent unit tests cover camera movement and finite projection, diagonal normalization, bounded delta, pitch/aspect guards, input transitions and focus reset, platform-key translation, deterministic surface-option selection, signed chunk model translation, stable/distinct diagnostic voxel colors, exact GPU payload sizing, and the M3B2 streaming bridge (see below). CI uses plain `cargo nextest run --workspace`; an accidentally empty suite is a failure.

GPU/window behavior remains a separate Windows host smoke test because CI must not require a graphical adapter. Coverage, property tests, fuzzing, automated visual regression, and performance benchmarks remain **NOT YET APPLICABLE**, not “passing.”

## Current M2 applicability

Eleven dependency-free voxel tests cover chunk strides and index uniqueness, bounds versus air, mutation and solid-count invariants, the locked diagnostic fixture/fingerprint and its exact 132/528/792 topology, empty/single/adjacent/solid chunks, all six face directions, internal-face removal, and triangle winding. The release `voxel-probe` reports topology, logical payload bytes, and one diagnostic CPU timing sample; it is not a benchmark or regression threshold. GPU/window behavior is validated separately by the Windows host smoke test.

## Current M3A applicability

Twenty dependency-free voxel tests now additionally lock Euclidean world/chunk/local conversion (including negatives and range extremes), checked axial-neighbor overflow, explicit known-versus-missing sampling, both boundary policies, solid/solid and solid/AIR seams in all six directions, canonical fixture order/fingerprint, and aggregate 202/808/1,212 topology. Existing M2 tests remain unchanged in meaning. The Windows smoke separately covers signed placement, visible external boundaries, seam appearance/culling, camera traversal, resize, minimize/restore, focus loss, and clean Escape shutdown.

## Current M3B applicability

Twenty-two voxel tests include owned slab sampling in every face direction and exact topology equivalence between the borrowed M3A neighborhood and the owned snapshot mesher. Thirty-one streaming tests cover deterministic demand sets, signed movement and oscillation, valid/invalid configuration (retention covering the halo, checked radius addition, zero budgets, oversized radius, `render ⊆ dependency ⊆ retention`), CPU-only dependency retention, cap reservation and cap-safe teleport, bounded eviction finalization, a full backlog refusing loads until released, cap invariance while a real-worker teleport backlog drains, request-token ABA/overflow, stale load and mesh stamps, source absence versus temporary unavailability, `KnownAbsent` render chunks never reporting a pending mesh, retention-only chunks never being offered for rendering, neighbor arrival, unload during work, load/mesh fairness, the finite diagnostic source, the real worker path, and clean shutdown.

Nine client streaming tests run against an in-memory `ChunkPresentation` double with the real runtime and worker: floor-based camera anchoring at `0`, `31.999`, `32`, `-0.001`, `-1`, `-32`, `-32.001`; explicit rejection of `NaN`, `±inf`, and out-of-range positions; center changes only across chunk boundaries with rejected anchors keeping the previous center; upload-budget admission; the settled draw set equalling the render-demand ready set with no dependency/retention chunk drawn and no re-upload of an identical stamp; leaving render demand deactivating every stale mesh in one update while releases drain under budget; re-entry rebuilding with new request tokens; and oversized meshes uploading alone while deferring the rest. Worker-timed tests poll with a 50 µs sleep because a tight `yield_now` loop can outrun thread wake-up on Windows. M3C0 adds ten voxel tests (32 in the crate): coarse constants, 16-edge bounds/read/write/strides, empty/full downsample, one-voxel features surviving, majority material with lowest-ID tie, determinism on the negative-coordinate fixture, 16-edge meshing, coarse slabs and seams in all six directions with borrowed/owned equality, a missing coarse neighbor rejected, and the locked coarse diagnostic topology (16 solids, 82 quads). All 86 workspace tests are headless. The Windows/D3D12 smoke is a separate, scripted step on the audited host: injected input, window operations, screenshot inspection, and log invariants, recorded per milestone in the planning document. The release `streaming-probe` is diagnostic evidence, not a benchmark threshold.
