# Current handoff

Last updated: 2026-09-16

## Current position

M0 bootstrap is complete: source preserved, documentation extracted, governance/ADRs established, host audited, Rust pinned, minimal dependency-free workspace and CI configured, and baseline checks run. No game systems exist.

## Continue here

**M1 — Rendering Foundation** is active on `feat/m1-rendering-foundation`. Its bounded plan and verification record belong in [`planning/M1_RENDERING_FOUNDATION.md`](../planning/M1_RENDERING_FOUNDATION.md). Keep implementation limited to window, adapter/device/surface, frame loop, input, camera, diagnostic primitive, shutdown, and diagnostics.

## Read before M1

- [`../../AGENTS.md`](../../AGENTS.md)
- [`../engineering/ARCHITECTURE.md`](../engineering/ARCHITECTURE.md)
- [`../engineering/INVARIANTS.md`](../engineering/INVARIANTS.md)
- [`../engineering/PERFORMANCE.md`](../engineering/PERFORMANCE.md)
- [`../audiovisual/ART_DIRECTION.md`](../audiovisual/ART_DIRECTION.md)
- ADRs 0001 and 0002

## Immediate risks

- Do not turn the documented future crate map into empty crates.
- Do not start voxel/worldgen inside M1.
- Treat integrated Intel Iris Xe as one conservative host, not the target matrix.
- Do not change repository visibility, replace/configure remotes, publish releases, choose a license, or present the working title as cleared without owner authorization.
