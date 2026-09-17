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

M3B2 adds per-frame presentation rails: one non-blocking runtime `poll` per frame (at most 4 results integrated, one job dispatched, at most 8 CPU evictions finalized), at most 2 GPU uploads per frame under a 4 MiB soft byte limit with a single oversized mesh allowed alone, and at most 8 buffer releases per frame. Drawability is decided every frame without budget; only physical release is budgeted. The frame callback performs no snapshot construction, disk, or network work; snapshot copies happen inside the runtime `poll` before rendering. The first release client interval on the audited host held ≈58 FPS under `Fifo` vsync while uploading 9 chunk meshes (2,214,144 GPU bytes) and settling 81 tracked records; a subsequent camera move produced 14 CPU evictions and 12 GPU releases without stale results or cap blocks. The driven smoke held 60.0 FPS (16.66–16.67 ms average wall frame) in every five-second traversal interval across 853 loads, 212 meshes, 106 uploads, and 319 evictions, with peak resident payload 3,997,696 bytes and peak GPU residency 2,223,504 bytes. Frame time is dominated by vsync in this scene, so these numbers bound nothing; capture integration and upload timing separately before changing any rail.

M3C2 measured one coarse level against a `Lod0`-only baseline at the same radius-3 visible distance on the same driven path (see the M3C2 table in the milestone document): resident GPU bytes fell 57% (12,101,888 → 5,206,128 peak) and quads likewise, but total upload bytes rose 74% (39,053,296 → 67,864,768) and CPU mesh time on the path rose 41% (274,069 → 386,583 µs) because band crossings swapped 618 chunk levels and issued 1,262 mesh jobs against 473. Both profiles held 60 FPS under vsync; render-submit timing was inconclusive there. By the recorded rule LOD is not the default. The M3B and M3C profiles are selectable with `VELDWAKE_PROFILE`.

The M3C2 follow-up (transition model) re-measured the same path on the same day: presentation gaps 909 → 0 for `m3c-banded`, with 648 level swaps, 1,388 mesh jobs, 548 uploads / 76,214,896 bytes (baseline 167 / 41,275,744), largest transition group 78 chunks, CPU mesh time on the path 380,377 µs against a same-day baseline of 164,698 µs (baseline totals varied 164,698–274,069 µs across runs), time to idle 13.3 s for both, render-submit max 17.8 ms, no GPU validation error. The upload-bytes rule still fails, so LOD remains opt-in; the model buys correctness of transitions, not throughput.

**GPU memory, corrected.** That run reported peak GPU bytes of 4,954,960 against 12,101,888, a −59% claim that counted committed meshes only. An atomic transition holds the staged replacement and the drawn original at the same time, so the high-water mark is the highest simultaneous `committed + staged` in one sample. Re-measured on the same path with the views off: baseline peak total 12,101,888 (committed 12,101,888, staged 0); banded peak total **9,800,128** (committed 5,640,752, staged 4,650,960), a **−19.0%** saving. The two component peaks sum to 10,291,712, above the real peak, because they never occur in the same frame. Same run: 543 uploads / 74,770,896 bytes against 167 / 41,275,744 (+81%), 1,376 mesh jobs against 501, CPU mesh time 408,525 µs against 171,199 µs (+139%), both profiles 60.0 FPS at 16.67 ms, no GPU validation error, `presentation_commit_failures` and `commit_invariant_failures` both zero. Sampling the presentation once per update costs nothing measurable at this scale.

**Debug views (M3C3).** Measured with an idle camera and no screen capture, banded profile, 12-second windows: `Off` and `Lod` hold 60.0 FPS at 16.66 ms and issue no debug draw; `Boundaries` at 180 primitives runs 46.7–47.9 FPS (20.9–21.4 ms); `Residency` at 637 boxes runs 37.6 then 15.4 FPS (26.6 then 64.8 ms). One uniform buffer, one bind group, and one draw per primitive is the cost (KI-012). With the views off the client returns to exactly 16.67 ms, so benchmarks on this path stay comparable.
