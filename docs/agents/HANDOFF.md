# Current handoff

Last updated: 2026-09-16

## Current position

M1 is merged into `main`. M2 implementation and local validation are complete on `feat/m2-voxel-prototype`, pending remote PR validation and merge. The dependency-free `veldwake-voxel` crate owns CPU chunk/mesh contracts; the client consumes one immutable mesh, assigns diagnostic colors, and owns the GPU buffers. No authoritative game state exists in the renderer.

## Continue here

Follow [`planning/M2_VOXEL_PROTOTYPE.md`](../planning/M2_VOXEL_PROTOTYPE.md). Validate and merge the M2 PR before selecting or starting M3 work. Preserve the tested CPU/presentation boundary; do not fold GPU color or buffer types into the voxel crate. Streaming, neighbors, seams, world coordinates, world generation, LOD, persistence, and multiple chunks are still absent.

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
- Treat integrated Intel Iris Xe as one conservative host, not the target matrix.
- Do not change repository visibility, replace/configure remotes, publish releases, choose a license, or present the working title as cleared without owner authorization.
