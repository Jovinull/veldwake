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

Fourteen GPU-independent unit tests cover camera movement and finite projection, diagonal normalization, bounded delta, pitch/aspect guards, input transitions and focus reset, platform-key translation, deterministic surface-option selection, signed chunk model translation, and stable/distinct diagnostic voxel colors. CI uses plain `cargo nextest run --workspace`; an accidentally empty suite is a failure.

GPU/window behavior remains a separate Windows host smoke test because CI must not require a graphical adapter. Coverage, property tests, fuzzing, automated visual regression, and performance benchmarks remain **NOT YET APPLICABLE**, not “passing.”

## Current M2 applicability

Eleven dependency-free voxel tests cover chunk strides and index uniqueness, bounds versus air, mutation and solid-count invariants, the locked diagnostic fixture/fingerprint and its exact 132/528/792 topology, empty/single/adjacent/solid chunks, all six face directions, internal-face removal, and triangle winding. The release `voxel-probe` reports topology, logical payload bytes, and one diagnostic CPU timing sample; it is not a benchmark or regression threshold. GPU/window behavior is validated separately by the Windows host smoke test.

## Current M3A applicability

Twenty dependency-free voxel tests now additionally lock Euclidean world/chunk/local conversion (including negatives and range extremes), checked axial-neighbor overflow, explicit known-versus-missing sampling, both boundary policies, solid/solid and solid/AIR seams in all six directions, canonical fixture order/fingerprint, and aggregate 202/808/1,212 topology. Existing M2 tests remain unchanged in meaning. The Windows smoke separately covers signed placement, visible external boundaries, seam appearance/culling, camera traversal, resize, minimize/restore, focus loss, and clean Escape shutdown.

## Current M3B1 applicability

Twenty-two voxel tests include owned slab sampling in every face direction and exact topology equivalence between the borrowed M3A neighborhood and the owned snapshot mesher. Twenty streaming tests cover deterministic demand sets, signed movement and oscillation, CPU-only dependency retention, cap reservation and cap-safe teleport, request-token ABA/overflow, stale load and mesh stamps, source absence versus temporary unavailability, neighbor arrival, unload during work, load/mesh fairness, the finite diagnostic source, the real worker path, and clean shutdown. These are headless; M3B1 adds no camera, window, or GPU test requirement. The release `streaming-probe` is diagnostic evidence, not a benchmark threshold.
