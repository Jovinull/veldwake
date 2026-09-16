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

## Current M1 applicability

Twelve GPU-independent unit tests cover camera movement and finite projection, diagonal normalization, bounded delta, pitch/aspect guards, input transitions and focus reset, platform-key translation, and deterministic surface-option selection. CI uses plain `cargo nextest run --workspace`; an accidentally empty suite is a failure.

GPU/window behavior remains a separate Windows host smoke test because CI must not require a graphical adapter. Coverage, property tests, fuzzing, automated visual regression, and performance benchmarks remain **NOT YET APPLICABLE**, not “passing.”

## Current M2 CPU applicability

Ten dependency-free unit tests cover chunk strides and index uniqueness, bounds versus air, mutation and solid-count invariants, the locked diagnostic fixture/fingerprint, exact topology for empty/single/adjacent/solid chunks, all six face directions, internal-face removal, and triangle winding. The release `voxel-probe` reports topology, logical payload bytes, and one diagnostic CPU timing sample; it is not a benchmark or regression threshold. GPU integration and visual validation remain pending.
