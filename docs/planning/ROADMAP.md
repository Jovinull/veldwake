# Capability roadmap

Status: **Proposed sequencing; no calendar estimates**.

Milestones are evidence gates, not dates. Each milestone must leave a small working capability, documented boundaries, representative tests, and measured risks.

## M0 — Repository & Engineering Foundation

Source preservation, environment audit, Git/workspace/toolchain, governance, memory, ADRs, quality gates, CI, and structured vision. **Complete.**

## M1 — Rendering Foundation

Window, adapter/device/surface, D3D12-first backend reporting, resize/lifecycle, frame loop, input, simple camera, clear diagnostic output, clean shutdown, and GPU-independent boundary tests. No voxel world.

**Complete and merged through PR #1:** see [`M1_RENDERING_FOUNDATION.md`](M1_RENDERING_FOUNDATION.md) for implementation and validation evidence.

## M2 — Voxel Prototype

Explicit voxel/chunk representation, one deterministic fixture chunk, edit/read API, selected baseline meshing technique, upload/render path, bounds/correctness tests, and CPU/mesh metrics.

**Complete and merged through PR #2:** the CPU/headless chunk, deterministic fixture, exposed-face reference mesher, one-time GPU upload/render path, tests, metrics, and Windows/D3D12 smoke evidence are present. See [`M2_VOXEL_PROTOTYPE.md`](M2_VOXEL_PROTOTYPE.md).

## M3 — Streaming World

Chunk coordinates, prioritized/cancellable generation and meshing jobs, residency/streaming budgets, initial LOD strategy, cache/persistence experiment, and debug visualization. No full civilization simulation.

**M3A — Multi-chunk Correctness is complete and merged through PR #3:** signed chunk/world conversion, explicit neighbor-aware seam meshing, deterministic adjacent-chunk fixtures, six-direction seam tests, and a small static rendered chunk set are present.

**M3B — Streaming Runtime is complete and merged through PR #5:** M3B1 provides deterministic bounded demand/residency, validated configuration, bounded eviction, immutable center-plus-slab meshing snapshots, request-token and generation-stamped stale-result rejection, one bounded standard-library worker, a finite diagnostic chunk source, and headless metrics/tests. M3B2 drives demand from the camera with floor-based anchoring, reconciles the GPU draw set every frame before budgeted uploads and releases, and reports aggregate diagnostics. Gates and the driven Windows/D3D12 smoke passed before merge. M3B deliberately excluded product world generation, saves, LOD, ECS, gameplay, physics, networking, and biomes. See [`M3_STREAMING_WORLD.md`](M3_STREAMING_WORLD.md).

**M3C — Initial LOD + Streaming Debug Visualization is complete and merged through [PR #6](https://github.com/Jovinull/veldwake/pull/6)** at merge commit `c669929b00427c2f438529b572931400a24b6d3d`, after independent branch QA and green remote CI. It delivered an edge-generic dense grid and mesher, one coarse 2× level with tested mixed-resolution seams, chunk-unit hysteresis, atomic presentation transitions, baseline/banded evidence, and keyboard debug views without a UI framework. LOD stays opt-in because upload bytes rise 81.6% while mutation-boundary accounting shows only a 16.9% presentation-owned chunk-mesh high-water saving. QA also records bounded ready-frontier coverage deficits rather than claiming zero holes. See [`M3_STREAMING_WORLD.md`](M3_STREAMING_WORLD.md).

**M3D — Disk Cache & Persistence Experiment is implemented and independently QA-hardened on `feat/m3d-cache-persistence`:** an explicitly versioned, endian-fixed discardable cache format; bounded hostile-input reads; behavioral source identity; `KnownAbsent` distinct from a miss and an empty chunk; best-effort recovery with observable cleanup failure; same-directory temp-and-rename with honest cross-platform concurrency semantics; and separate aggregate metrics. Cold open scans run once during opt-in initialization, while per-chunk I/O stays on the worker and no I/O enters the frame hot path. Raw remains default; both raw and RLE remain readable regardless of current write preference. The cache is slower than regeneration for the trivial diagnostic source (KI-016), has no eviction policy (KI-015), and is not a save. With M3D, **M3 is technically complete on its branch** and awaits external review/PR before it counts as merged. See the M3D section of [`M3_STREAMING_WORLD.md`](M3_STREAMING_WORLD.md).

## M4 — Beautiful Terrain Vertical Slice

One coherent region containing terrain, biome, hydrology cues, vegetation, sky, sunlight/shadows, atmosphere/fog, water/weather subset, stable visual fixtures, and target-host performance capture.

## M5 — Procedural Character

One humanoid descriptor/compiler path, consistent voxel geometry/materials, generated skeleton, locomotion, terrain contact/IK subset, collision representation, preview/fixture pipeline, and style-rule evidence.

## M6 — Combat Slice

One weapon and one enemy with movement, attack/defense or dodge, telegraphs, hit reaction, camera response, procedural impact audio, VFX, animation, and encounter/readability playtest evidence.

## Later capability groups

Persistence/world editing, aggregate/local world simulation, settlements/history/economy, richer procedural assets/audio/music, multiplayer transport, and WASM modding follow only after the central technical and fun risks are proven. Split and order them when earlier evidence exists; do not manufacture detailed milestones now.

## Milestone exit rule

Exit requires demonstrable capability, applicable gates, current docs/state/handoff, resolved or accepted risks, and explicit exclusions for the next scope. A screenshot without tests/metrics is not a renderer milestone; infrastructure without a usable slice is not product progress.
