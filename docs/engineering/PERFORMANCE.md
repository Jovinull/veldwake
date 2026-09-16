# Performance policy

Status: **Accepted method; numeric budgets TBD**.

Performance claims require a workload, hardware/configuration, build profile, metrics, and captured result. “Fast” is not evidence.

## Frame-critical principles

- No blocking disk/network I/O on frame hot paths.
- Bound generation, meshing, uploads, simulation, and save work per frame/tick.
- Prefer batches and coherent data layouts; avoid global locks and avoid allocations per frame when measurement shows them material.
- Keep render, simulation, and asynchronous workloads observable separately.
- Use LOD/streaming for geometry and simulation; distant mountains are not full voxel grids and distant NPCs are not full entities.
- Optimize the dominant bottleneck; do not micro-optimize cold setup code.

## Candidate techniques by maturity

Near-term voxel work may evaluate chunking, greedy meshing or alternatives, frustum culling, asynchronous generation/meshing, distance LOD, palette/region compression, and upload budgets. Occlusion, GPU indirect rendering, meshlets, compute culling, hierarchical depth, and GPU vegetation/particles require later evidence.

## Measurement record

Benchmarks/profiles should capture commit, tool version, hardware, driver/backend, release flags, fixture/seed, warm-up, sample count, percentiles, memory, and interpretation. Regression thresholds must account for noise and run on stable fixtures.

The 1080p/60 target from concept work remains Proposed until a target hardware tier and representative scene define CPU/GPU/memory budgets.

## Current instrumentation

The client records wall-clock presentation delta, clamps camera movement after stalls, and emits a five-second aggregate of observed frame interval/FPS through `tracing`. This is diagnostic telemetry, not a performance benchmark or evidence for the proposed target. The event loop uses `ControlFlow::Wait`: visible rendering continues by chaining `request_redraw()`, while occlusion stops that chain until restoration explicitly requests another redraw. The frame callback performs no disk/network I/O and allocates no unbounded per-frame work.

M2 constructs and meshes the one diagnostic chunk once during startup, then creates immutable GPU buffers (12,672 vertex bytes and 3,168 index bytes). It does not remesh or upload voxel geometry per frame. The release probe remains the CPU measurement source; one 2026-09-16 run measured the diagnostic fixture at 36 µs, which is an observation rather than a budget.

M3A extends the same one-time path to three static chunks without introducing jobs or streaming. Neighbor-aware meshing measured 76 µs in the final release probe run on the audited host and produced 202 quads. The client uploaded 19,392 vertex bytes, 4,848 index bytes, and 48 bytes of model uniforms once. Per-chunk buffers/bind groups and linear fixture neighbor lookup are correctness-proof choices, not scale claims or accepted future budgets.

M3B1 bounds raw resident payloads plus eviction payloads plus reserved loads before dispatch. The default hard cap is 160 dense chunks (10 MiB raw voxel payload); it is a diagnostic limit, not a shipping budget. Detached mesh snapshots copy at most 76 KiB, queues hold descriptors rather than snapshots, and only one job is in flight. Release probe runs on 2026-09-16 reached idle in 5,163–7,660 µs with 63 resident payloads, 27 meshes, and 2,064,384 cumulative snapshot bytes. This one-host observation is neither a latency target nor evidence for adding workers.

M3B2 adds per-frame presentation rails: one non-blocking runtime `poll` per frame (at most 4 results integrated, one job dispatched, at most 8 CPU evictions finalized), at most 2 GPU uploads per frame under a 4 MiB soft byte limit with a single oversized mesh allowed alone, and at most 8 buffer releases per frame. Drawability is decided every frame without budget; only physical release is budgeted. The frame callback performs no snapshot construction, disk, or network work; snapshot copies happen inside the runtime `poll` before rendering. The first release client interval on the audited host held ≈58 FPS under `Fifo` vsync while uploading 9 chunk meshes (2,214,144 GPU bytes) and settling 81 tracked records; a subsequent camera move produced 14 CPU evictions and 12 GPU releases without stale results or cap blocks. Frame time is dominated by vsync in this scene, so these numbers bound nothing; capture integration and upload timing separately before changing any rail.
