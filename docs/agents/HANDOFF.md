# Current handoff

Last updated: 2026-09-16

## Current position

M1 is complete and merged into `main` through PR #1 at `ef653238be903daa6a1cbf74a9e31e3465cf8b57`. M2 planning is active on `feat/m2-voxel-prototype`; no voxel code has been implemented. The workspace contains the dependency-free `foundation` crate and one Windows-first presentation client, with no authoritative game state in the renderer.

## Continue here

Follow [`planning/M2_VOXEL_PROTOTYPE.md`](../planning/M2_VOXEL_PROTOTYPE.md). First establish the GPU-independent voxel/chunk contract, checked indexing, and deterministic fixture with headless tests. Only then implement and measure the baseline mesher, followed by the smallest client upload/render adapter. Do not begin streaming, world generation, LOD, persistence, or multiple-chunk orchestration.

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
- Treat integrated Intel Iris Xe as one conservative host, not the target matrix.
- Do not change repository visibility, replace/configure remotes, publish releases, choose a license, or present the working title as cleared without owner authorization.
