# Current handoff

Last updated: 2026-09-16

## Current position

M1 and M2 are merged into `main`; M2 landed through PR #2 at merge commit `b7f7461911e2f5c832dd9dae475ebb375dda870e`. M3A planning is active on `feat/m3-multichunk-foundation`, with no M3A implementation yet. The dependency-free `veldwake-voxel` crate owns CPU chunk/mesh contracts; the client owns diagnostic colors, transforms, and GPU buffers. No authoritative game state exists in the renderer.

## Continue here

Follow the M3A proposal in [`planning/M3_STREAMING_WORLD.md`](../planning/M3_STREAMING_WORLD.md). Implement the signed coordinate contract and headless seam tests before extending the diagnostic client to a small static chunk set. Preserve local mesh coordinates and apply per-chunk presentation offsets outside the voxel mesh. Do not begin streaming, jobs, world generation, LOD, persistence, or generalized world orchestration.

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
- The current outside-as-air rule is local to the isolated M2 reference mesher; it is not a future seam/neighborhood policy.
- Negative world coordinates require Euclidean division, not truncating integer division; lock boundary cases before depending on the conversion.
- Missing neighbors in a static M3A set are presentation boundaries, but a future streaming system must not silently equate “not loaded” with AIR.
- Treat integrated Intel Iris Xe as one conservative host, not the target matrix.
- Do not change repository visibility, replace/configure remotes, publish releases, choose a license, or present the working title as cleared without owner authorization.
