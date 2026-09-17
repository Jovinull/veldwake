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

Fifty-three GPU-independent client tests cover camera/input, surface selection, signed transforms, GPU payload sizing, streaming bridge transactions/coverage metrics, and debug-state mapping/geometry. CI uses plain `cargo nextest run --workspace`; an accidentally empty suite is a failure.

GPU/window behavior remains a separate Windows host smoke test because CI must not require a graphical adapter. Coverage, property tests, fuzzing, automated visual regression, and performance benchmarks remain **NOT YET APPLICABLE**, not “passing.”

## Current M2 applicability

Eleven dependency-free voxel tests cover chunk strides and index uniqueness, bounds versus air, mutation and solid-count invariants, the locked diagnostic fixture/fingerprint and its exact 132/528/792 topology, empty/single/adjacent/solid chunks, all six face directions, internal-face removal, and triangle winding. The release `voxel-probe` reports topology, logical payload bytes, and one diagnostic CPU timing sample; it is not a benchmark or regression threshold. GPU/window behavior is validated separately by the Windows host smoke test.

## Current M3A applicability

Twenty dependency-free voxel tests now additionally lock Euclidean world/chunk/local conversion (including negatives and range extremes), checked axial-neighbor overflow, explicit known-versus-missing sampling, both boundary policies, solid/solid and solid/AIR seams in all six directions, canonical fixture order/fingerprint, and aggregate 202/808/1,212 topology. Existing M2 tests remain unchanged in meaning. The Windows smoke separately covers signed placement, visible external boundaries, seam appearance/culling, camera traversal, resize, minimize/restore, focus loss, and clean Escape shutdown.

## Current M3B applicability

Twenty-two voxel tests include owned slab sampling in every face direction and exact topology equivalence between the borrowed M3A neighborhood and the owned snapshot mesher. Thirty-one streaming tests cover deterministic demand sets, signed movement and oscillation, valid/invalid configuration (retention covering the halo, checked radius addition, zero budgets, oversized radius, `render ⊆ dependency ⊆ retention`), CPU-only dependency retention, cap reservation and cap-safe teleport, bounded eviction finalization, a full backlog refusing loads until released, cap invariance while a real-worker teleport backlog drains, request-token ABA/overflow, stale load and mesh stamps, source absence versus temporary unavailability, `KnownAbsent` render chunks never reporting a pending mesh, retention-only chunks never being offered for rendering, neighbor arrival, unload during work, load/mesh fairness, the finite diagnostic source, the real worker path, and clean shutdown.

Client streaming remains tested against an in-memory `ChunkPresentation` double with the real runtime and worker: camera anchoring, upload/release budgets, draw-set reconciliation, transitions, atomic group commit, coverage classification, LOD, debug modes, and restage order all stay headless. GPU-specific renderer behavior remains covered by contract tests plus the recorded D3D12 milestone smoke.

M3D has 40 headless cache tests. They cover layout/endianness; raw and RLE round trips; empty chunk versus known absence versus missing file; every short header; hostile raw/RLE lengths and runs; arbitrary coordinates and voxel IDs; checksum/identity/version rejection, including a damaged structural field classifying as corrupt while a valid resealed foreign field is stale; exact raw, absence, and 3× worst-case RLE sizes; bounded oversized-file reads; traversal-free names; normal and concurrent publication; fingerprint isolation; temporary sweeping; cold/warm behavior; cleanup/write failures preserving source results; repeated poisoned-entry observability; raw↔RLE preference changes; clearing; exhaustive finite-source behavioral identity; stale token rejection with cache accounting preserved; and cacheless counters remaining zero. `CacheFormatError` is private because no external crate encodes or decodes entries. The workspace has 184 tests (38 voxel, 90 streaming, 56 client); all 144 pre-M3D tests remain. Retries are permitted only for KI-008/LNK1104, never functional failure. `streaming-probe` is diagnostic evidence, not a benchmark threshold.
