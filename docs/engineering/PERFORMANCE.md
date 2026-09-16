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

M1 records wall-clock presentation delta, clamps camera movement after stalls, and emits a five-second aggregate of observed frame interval/FPS through `tracing`. This is diagnostic telemetry for a trivial cube under a polling event loop, not a performance benchmark or evidence for the proposed target. The frame callback performs no disk/network I/O and allocates no unbounded per-frame work.
