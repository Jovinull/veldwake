# Current handoff

Last updated: 2026-09-16

## Current position

M1 and M2 are merged into `main`; M2 landed through PR #2 at merge commit `b7f7461911e2f5c832dd9dae475ebb375dda870e`. M3A is implemented and locally validated on `feat/m3-multichunk-foundation`, pending review/PR and merge. The dependency-free `veldwake-voxel` crate owns CPU chunk/coordinate/neighborhood/mesh contracts; the client owns diagnostic colors, per-chunk transforms, and GPU buffers. No authoritative game state exists in the renderer.

## Continue here

Review the M3A implementation and evidence in [`planning/M3_STREAMING_WORLD.md`](../planning/M3_STREAMING_WORLD.md), then open a PR when authorized. Do not begin M3B merely because M3A is coded: streaming/jobs require a separate scope after M3A review and merge. Preserve the explicit missing-neighbor policy and the boundary in which CPU meshes stay local while the client supplies diagnostic transforms.

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
- Treat integrated Intel Iris Xe as one conservative host, not the target matrix.
- Do not change repository visibility, replace/configure remotes, publish releases, choose a license, or present the working title as cleared without owner authorization.
