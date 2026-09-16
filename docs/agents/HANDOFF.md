# Current handoff

Last updated: 2026-09-16

## Current position

M1, M2, and M3A are merged into `main`; M3A landed through PR #3 at merge commit `af1cabc913a9500eefbcd647a56c881d2c1288c0`. M3B planning is active on `feat/m3b-streaming-runtime`, with no M3B implementation. The dependency-free `veldwake-voxel` crate owns CPU chunk/coordinate/neighborhood/mesh contracts; the client owns diagnostic colors, per-chunk transforms, and GPU buffers. No authoritative game state exists in the renderer.

## Continue here

Review the proposed M3B scope in [`planning/M3_STREAMING_WORLD.md`](../planning/M3_STREAMING_WORLD.md) before implementation. The current recommendation is a CPU streaming runtime outside the renderer, one bounded standard-library worker, and owned 64 KiB center plus six 2 KiB face slabs per dispatched mesh snapshot. Validate or revise those proposed decisions with evidence; do not implement product world generation, saves, LOD, ECS, gameplay, physics, networking, or biomes as part of M3B.

## Read before continuing

- [`../../AGENTS.md`](../../AGENTS.md)
- [`../engineering/ARCHITECTURE.md`](../engineering/ARCHITECTURE.md)
- [`../engineering/INVARIANTS.md`](../engineering/INVARIANTS.md)
- [`../engineering/PERFORMANCE.md`](../engineering/PERFORMANCE.md)
- [`../audiovisual/ART_DIRECTION.md`](../audiovisual/ART_DIRECTION.md)
- ADRs 0001 and 0002

## Immediate risks

- Do not turn the documented future crate map into empty crates.
- Do not let the M2 voxel representation or mesher depend on `wgpu`, `winit`, or the diagnostic camera.
- The M1 cube has been replaced by the M2 fixture; its palette and framing remain diagnostic presentation, not game art direction.
- Keep chunk dimensions, material encoding, coordinate order, and mesh winding explicit and tested; accidental conventions will become expensive compatibility constraints.
- The M2 convenience mesher explicitly uses `BoundaryPolicy::Expose`; future streaming work must use deliberate availability policy rather than silently equating “not loaded” with AIR.
- Negative world coordinates use Euclidean division and are locked at `0`, `31`, `32`, `-1`, `-32`, `-33`, plus range extrema. Preserve this contract.
- One model uniform/bind group and a linear fixture lookup per chunk are intentionally limited M3A diagnostics, not accepted scalable batching or residency designs.
- M3B must not equate an unavailable/loading neighbor with known AIR. Proposed jobs carry center/neighbor generations and results are accepted only if every stamp remains current.
- Proposed M3B budgets and radii are diagnostic starting limits, not target-world performance promises. Measure before expanding workers, residency, or upload throughput.
- Treat integrated Intel Iris Xe as one conservative host, not the target matrix.
- Do not change repository visibility, replace/configure remotes, publish releases, choose a license, or present the working title as cleared without owner authorization.
