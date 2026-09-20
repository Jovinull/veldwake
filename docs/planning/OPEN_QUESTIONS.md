# Open questions

Questions are decisions not safely inferable from the transcript. Resolve at the named decision horizon, not all at once.

## Owner/product decisions

1. **Project license and contribution model** — required before public distribution or accepting external contributions.
2. **Name clearance** — is the owner prepared to commission formal Veldwake trademark/domain/store checks, and in which jurisdictions?
3. **Commercial/distribution intent** — premium, early access, open source, source-available, or undecided; affects licensing/mod/service plans.
4. **Primary early audience and accessibility baseline** — keyboard/mouse versus controller priority, localization, motion/visual/audio accessibility.

## Before or during M1

5. Define the initial supported Windows floor and target hardware tier; turn 1080p/60 into measurable frame/memory budgets.
6. Decide whether M1 needs a tool UI immediately or only logs/debug text; `egui` is not pre-approved as a dependency.
7. Define CI Windows coverage versus faster Linux checks once native graphics dependencies enter.

## Before voxel/world milestones

8. Select voxel storage unit/scales, chunk dimensions, edit granularity, terrain topology, meshing baseline, and coordinate precision through prototypes. Everything here is settled except **edit granularity**, which remains untouched: nothing in the game can change a voxel.
9. Decide finite-world topology/size and world-version compatibility policy.
10. Define the first STYLE_BIBLE with concrete shape/palette/material examples and review ownership.
11. Decide persistence guarantees for edited/generated chunks and recovery expectations. **Still open after M7 and now the nearest one**: a session leaves no trace at all, and M7 deliberately added none.

## Before gameplay/product slices

12. Combat camera, controls, defense/dodge/stamina model, difficulty/accessibility, and first weapon/enemy.
13. Character creation versus generated identity, death/failure loop, and progression reset/respec policies. **Still open after M7.** M7 chose the smallest behaviour that keeps a session alive and nothing more: a defeated player returns to its configured start after the existing hold, and a defeated adversary stays where it fell. That is not a death system, a respawn system or a failure loop, and it decides nothing about what one should be.
14. Minimum construction/world-editing scope and interaction with NPC rebuilding.
15. Co-op player count, hosting model, griefing/ownership, offline progression, and PvP stance.
16. Mod API scope, trust/signing/discovery, server authority, and distribution policy.

Resolved questions move to the relevant canonical document and, when durable/structural, an ADR.
