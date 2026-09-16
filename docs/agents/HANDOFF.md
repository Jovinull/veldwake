# Current handoff

Last updated: 2026-09-16

## Current position

M1, M2, and M3A are merged into `main`; M3A landed through PR #3 at merge commit `af1cabc913a9500eefbcd647a56c881d2c1288c0`. M3B (M3B1 headless runtime plus M3B2 camera-driven GPU integration) is implemented on `feat/m3b-streaming-runtime` and awaits the owner's interactive smoke and review. `veldwake-voxel` owns CPU chunk/coordinate/neighborhood/snapshot/mesh contracts; `veldwake-streaming` owns headless residency, worker orchestration, and per-frame observability; the client's `streaming` module bridges camera demand to a `ChunkPresentation` implemented by the renderer, which owns only disposable GPU state keyed by `ChunkCoord`.

## Continue here

Perform the interactive Windows smoke listed under "M3B2 observed evidence" in [`planning/M3_STREAMING_WORLD.md`](../planning/M3_STREAMING_WORLD.md): traverse several positive and negative chunk boundaries, watch chunks appear/disappear without persistent seams or speculative AIR, confirm the camera never stalls, re-enter a left area, teleport far, and exercise resize/minimize/restore/focus loss/Escape while reading the five-second `M3B streaming state` / `M3B streaming work and budgets` lines. Then open the M3B pull request. Do not start M3C (LOD, cache/persistence experiment, debug visualization, worker scaling) or introduce product world generation, saves, ECS, gameplay, physics, networking, or biomes before M3B is accepted.

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
- M3B1 does not equate an unavailable/loading neighbor with known AIR. Jobs carry a global non-reused request token plus center/neighbor generations; results are accepted only while every stamp remains current.
- The default 27/81/125 demand counts, 160-payload cap, one worker, and observed probe timing are diagnostic evidence, not target-world performance promises. Measure before expanding workers, residency, or upload throughput.
- Keep drawability and deallocation separate in the bridge: a presented mesh whose stamp is no longer current is deactivated in the same update, before uploads and regardless of the release budget. Never reorder that to "upload first".
- `ChunkPresentation` has exactly two implementors (renderer, test double). It is a testability seam, not a render abstraction; do not add methods the bridge does not call.
- Camera anchoring must stay `floor`-based with explicit non-finite/out-of-range rejection; `as i64` truncates toward zero and would misplace every negative sub-voxel position.
- With one worker, `finalize_evictions` before `dispatch_one` means an eviction backlog cannot trigger `hard_cap_blocks`; the cap holds through accounting. Re-validate that reasoning if worker count or poll order ever changes.
- Treat integrated Intel Iris Xe as one conservative host, not the target matrix.
- Do not change repository visibility, replace/configure remotes, publish releases, choose a license, or present the working title as cleared without owner authorization.
