# M8 — Discoverable Landmarks

Status: **in progress on `feat/m8-discoverable-landmarks`.**
Base: `docs/post-m7-handoff` at `08081b8a15be3ea70de82e03cb0b113d71c7c8d9`, which is `main` at merge commit `0c81c069bb95a8caf3b6a5252b89b023c6334ae1` plus one documentation commit.

M7 proved a person can walk the whole region. It also proved, by accident and in the owner's own playtest, that walking a beautiful empty region is a walk and not a game: the owner crossed it, enjoyed crossing it, and never found the one thing standing in it. Today no two directions in Veldwake mean different things. M8 builds the first spatial choice: from a discovery overlook the player sees, in the world itself, more than one destination, chooses one with no navigation interface of any kind, walks to it, and finds the place the silhouette promised.

This document is written as the milestone lands, phase by phase. Anything not yet measured is not claimed, and every number in the design sections below is a hypothesis until a compiled fixture or a capture confirms it.

## Scope, as accepted by the owner

> "O que é aquilo lá?" O jogador começa num overlook, percebe pelo próprio mundo pelo menos dois destinos diferentes, escolhe um sem qualquer UI de navegação, caminha até ele e encontra o lugar que a silhueta prometeu.

M8 is **not** persistence, **not** world editing, **not** a spawn or encounter system, and **not** a navigation system. It is world content that makes a direction worth choosing.

## Owner decisions

| decision | ruling |
|---|---|
| name | M8 — Discoverable Landmarks |
| content | **generated world content**: not edited, not persisted, not save data. The same seed yields the same landmarks |
| count | **three** landmarks, **one** procedural family (Monolith), three silhouette classes: Spire, Gate, Broken |
| discovery | **100% diegetic.** No minimap, compass, marker, quest marker, waypoint, floating icon, breadcrumb or navigation HUD |
| first choice | at least **two** landmarks legible together from the discovery overlook region; the third may be revealed during traversal |
| enemy | the one existing adversary lives deterministically at **A or B**, the two initially discoverable landmarks — never at C. The owner is not told which |
| gate | **passable.** If the opening visually admits the body, traversal admits it |
| overlook clearing | the golden composition may use a reservation of about the measured `r ≈ 40` so that landmarks of about 24 units and taller read. **40 is a control of the golden plan, not a universal landmark rule**, and visual QA may reject the clearing if it reads as artificial |
| streaming | current envelope only: `render_radius = 6`, `CHUNK_EDGE = 32`, `Lod0Only`. No LOD rollout, no radius increase, no streaming rewrite. If evidence contradicts this, stop and report |
| exit gate | a **blind** OWNER PLAYTEST: the owner is told only "jogue/explore normalmente" |
| out of scope | persistence, world editing, inventory, loot, equipment, progression, quests, NPCs, a second enemy or weapon, a second biome or region, settlements, economy, history, day/night, running refinement, jump, climb, swim, fast travel, any navigation UI, generalized physics, generalized voxel collision, interiors, runtime pathfinding, ECS, mandatory LOD, streaming rewrite, multiplayer, WASM, a universal architecture generator |

## Architectural corrections from the implementation review

These were imposed on the approved design before any code was written. Each is a constraint the implementation answers to.

1. **No lazy plan inside the generator.** `TerrainGenerator` is cheap to clone and free of interior mutability, and `generate(coord) -> Option<Chunk>` has no channel for a planning failure. The landmark plan is therefore derived and validated **before** chunk generation: constructing the world fails with a typed error if no plan exists. Losing `Copy` is acceptable; hiding a fallible, expensive derivation in a `OnceLock` is not. If the cost matters, measure it and reuse the plan explicitly.
2. **Visibility in two levels.** `veldwake-procedural` owns a **world visibility proxy** used for placement: observer position, terrain and vegetation profiles, landmark silhouette, distance, geometric clearance. It knows nothing about field of view, pixels, the camera, the renderer or the fog shader. The **presentation visibility oracle** lives in the client, knows all of those, and validates placement against the real presentation. Math predicts; captures prove.
3. **One vegetation truth.** Once landmarks suppress plants, the raw vegetation grammar no longer describes the world. There is one canonical query of world vegetation — grammar plus landmark suppression — and every consumer that asks "is a plant actually here?" uses the same composition chunk generation uses.
4. **A discovery overlook, not a player arrival.** The world owns no spawn. The plan produces a `DiscoveryOverlook` as a compositional property of the world; the client uses it as the start of an M8 session. The golden controls may carry an honestly named composition hint near `(-69, 49)`.
5. **No body radius in the world.** The plan exposes exact landmark solid geometry. The client, which owns the character's collision representation, turns it into a keep-out for `TraversalLegality`. No character dimension appears in `veldwake-procedural`.

## Phases

| phase | contents | status |
|---|---|---|
| M8A | this document, `LANDMARK_STYLE.md` v1, owner decisions, scope and non-goals, the two-level visibility split, overlook semantics | complete |
| M8B | the pure Monolith family: materials, descriptor, canonicalization, compiler, occupancy, silhouette, identity | not started |
| M8C | the immutable world plan: `DiscoveryOverlook`, site placement, world visibility proxy, three sites, vegetation reservation, typed construction errors, determinism, cost | not started |
| M8D | chunk integration: suppressed vegetation, landmark voxels, identity and cache fingerprint, the global `VoxelId` lookup, order independence | not started |
| M8E | traversal integration: plan-aware world vegetation, keep-out from the real body radius, `SurfaceGrid`, audit, route, enemy host, gate passage, continuous route | not started |
| M8F | presentation and evidence: the client visibility oracle, real rendering, captures, motion, weather, performance | not started |
| M8G | technical self-QA: full regression, the real game, fresh captures, documentation. Then stop for the blind owner playtest | not started |

## The feasibility result this milestone starts from

The design round measured the golden world from outside the repository, through the public API of `veldwake-procedural`, with a reachability graph that reproduced the M7 audit's `307,162` forward columns exactly and a camera model that reproduced the logged follow-camera height of `22.5738` exactly.

- The current envelope is sufficient. `m4_golden` renders a Chebyshev cube of six chunks around the anchor chunk, which reaches **at least 192 world units in every horizontal direction**. Every viable site pair lies 60 to 170 units from where it is seen.
- **The blocker is near canopy.** The M7 arrival at `(-69, 49)` — the M5 portrait stand, the M6 arena centre and the M7 route start — sits in a `MeadowWood` pocket whose canopy, 6 to 18 units away, rises to 12.8–23.4 degrees in most directions. From that point no candidate site is framed at the default follow pitch, and the best distinct pair is framed from about 1% of the ground within 24 units.
- A vegetation reservation around the overlook resolves it. The best distinct pair, framed across the overlook's first 12 units:

| landmark height \ clearing radius | 24 | 32 | 40 |
|---|---|---|---|
| 20 | 8% | 8% | 57% |
| 24 | 24% | 35% | 98% |
| 28 | 69% | 76% | 98% |
| 32 | 98% | 98% | 100% |

- The occluder model was checked against five real frames from the arrival with the camera pose read from the client's own report: median error between predicted and measured sky boundary `0, 0, 0, 10, 0` pixels, and the model is conservative — it treats canopy as solid, the frames show gaps.

## Non-goals

Recorded here once so the rest of the document can refer to them: everything in the owner's out-of-scope row above, and in particular no walking on any landmark surface (roofs, lintels, rubble tops), no terrain excavation, no landmark material replacing a terrain voxel, and no change to `GroundSampler`, `check_move`, `MoveBlockReason`, the step and drop bounds, MOVE-001, combat tuning, the AI, the weapon, health, damage or the telegraph.
