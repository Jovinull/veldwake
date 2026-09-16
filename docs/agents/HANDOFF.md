# Current handoff

Last updated: 2026-09-16

## Current position

M1 implementation is complete on `feat/m1-rendering-foundation`, pending remote PR validation and merge into `main`. The workspace contains the unchanged dependency-free `foundation` crate and one Windows-first presentation client. The client renders a diagnostic cube through `wgpu`/D3D12, supports a pure camera/input model, handles window/surface lifecycle, and reports adapter/frame diagnostics. It contains no authoritative game state.

## Continue here

Finish remote review and merge of M1 before beginning M2. The implementation and verification record belongs in [`planning/M1_RENDERING_FOUNDATION.md`](../planning/M1_RENDERING_FOUNDATION.md). After merge, M2 should begin by specifying a minimal deterministic voxel/chunk fixture and CPU-side correctness contract before adding a mesher; preserve the tested rule that world/voxel data does not require a GPU and that the renderer only consumes presentation data.

## Read before continuing

- [`../../AGENTS.md`](../../AGENTS.md)
- [`../engineering/ARCHITECTURE.md`](../engineering/ARCHITECTURE.md)
- [`../engineering/INVARIANTS.md`](../engineering/INVARIANTS.md)
- [`../engineering/PERFORMANCE.md`](../engineering/PERFORMANCE.md)
- [`../audiovisual/ART_DIRECTION.md`](../audiovisual/ART_DIRECTION.md)
- ADRs 0001 and 0002

## Immediate risks

- Do not turn the documented future crate map into empty crates.
- Do not let the M2 voxel representation depend on `wgpu`, `winit`, or the diagnostic camera.
- Keep the cube clearly identified as disposable diagnostic content; do not evolve it into game content.
- Treat integrated Intel Iris Xe as one conservative host, not the target matrix.
- Do not change repository visibility, replace/configure remotes, publish releases, choose a license, or present the working title as cleared without owner authorization.
