# Current handoff

Last updated: 2026-09-16

## Current position

M0 bootstrap is complete: source preserved, documentation extracted, governance/ADRs established, host audited, Rust pinned, minimal dependency-free workspace and CI configured, and baseline checks run. No game systems exist.

## Continue here

The single recommended next milestone is **M1 — Rendering Foundation**. Before adding crates/dependencies, write a narrow M1 acceptance plan from [`planning/ROADMAP.md`](../planning/ROADMAP.md), confirm current `wgpu`/`winit`/`glam` versions and features from primary docs, then create only the boundaries required for window, adapter/device, frame loop, input, camera, shutdown, and diagnostic reporting.

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
- Do not create a remote, choose a license, or present the working title as cleared.
