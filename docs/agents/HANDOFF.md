# Current handoff

Last updated: 2026-09-16

## Current position

M1 is complete and merged into `main` through PR #1 at `ef653238be903daa6a1cbf74a9e31e3465cf8b57`. M2 is active on `feat/m2-voxel-prototype`. The dependency-free `veldwake-voxel` crate now contains the CPU/headless chunk, fixture, exposed-face reference mesher, tests, and release measurement probe. No voxel GPU integration or authoritative game state exists in the renderer.

## Continue here

Follow [`planning/M2_VOXEL_PROTOTYPE.md`](../planning/M2_VOXEL_PROTOTYPE.md). Preserve the tested CPU contract and implement only the smallest client adapter that uploads the immutable diagnostic mesh and renders the one fixture chunk. Do not redesign the reference mesher or begin streaming, world generation, LOD, persistence, or multiple-chunk orchestration.

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
- Keep the cube clearly identified as disposable diagnostic content; do not evolve it into game content.
- Keep chunk dimensions, material encoding, coordinate order, and mesh winding explicit and tested; accidental conventions will become expensive compatibility constraints.
- The current outside-as-air rule is local to the isolated M2 reference mesher; it is not a future seam/neighborhood policy.
- Treat integrated Intel Iris Xe as one conservative host, not the target matrix.
- Do not change repository visibility, replace/configure remotes, publish releases, choose a license, or present the working title as cleared without owner authorization.
