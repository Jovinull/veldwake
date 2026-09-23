# M8 — Discoverable Landmarks

Status: **complete and merged.** It landed through [PR #12](https://github.com/Jovinull/veldwake/pull/12) at merge commit `ee35f62f97afbe3d001a27a576e9bae21e77c4d2`, whose parents are `0c81c069bb95a8caf3b6a5252b89b023c6334ae1` and `d5e200bbd18d7c5aee167151509b89be260b85e2`, after the owner's discovery playtest, independent branch QA, a green pull-request CI run ([run 35732875867](https://github.com/Jovinull/veldwake/actions/runs/35732875867)) and a green post-merge CI run on the merge commit ([run 35735467195](https://github.com/Jovinull/veldwake/actions/runs/35735467195)). The remote branch `feat/m8-discoverable-landmarks` is preserved at `d5e200bbd18d7c5aee167151509b89be260b85e2`, the head where the 755 tests and every gate were run.
Base: `docs/post-m7-handoff` at `08081b8a15be3ea70de82e03cb0b113d71c7c8d9`, which is `main` at merge commit `0c81c069bb95a8caf3b6a5252b89b023c6334ae1` plus one documentation commit.
Branch state at the owner gate: head `d987ec6301460336b4214d1ee704d3ea77b936d7`, **four commits ahead of `main`** and **three after `docs/post-m7-handoff`** — the milestone plan, the implementation, and the documentation of it. A report that said "two commits" was counting one session's own commits, not the branch.

M7 proved a person can walk the whole region. It also proved, by accident and in the owner's own playtest, that walking a beautiful empty region is a walk and not a game: the owner crossed it, enjoyed crossing it, and never found the one thing standing in it. Today no two directions in Veldwake mean different things. M8 builds the first spatial choice: from a discovery overlook the player sees, in the world itself, more than one destination, chooses one with no navigation interface of any kind, walks to it, and finds the place the silhouette promised.

This document was written as the milestone landed, phase by phase. The design sections below are kept as they were written, including the predictions, and the implementation sections after them say what was actually measured — including the two places the measurement contradicted the design and what was chosen instead.

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
| exit gate | an OWNER PLAYTEST with **no navigation instructions**: the owner is told only "jogue/explore normalmente", and is given no coordinate, direction, waypoint or route. The owner had read the implementation report beforehand, so this is uninstructed rather than literally blind |
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
| M8B | the pure Monolith family: materials, descriptor, canonicalization, compiler, occupancy, silhouette, identity | complete |
| M8C | the immutable world plan: `DiscoveryOverlook`, site placement, world visibility proxy, three sites, vegetation reservation, typed construction errors, determinism, cost | complete |
| M8D | chunk integration: suppressed vegetation, landmark voxels, identity and cache fingerprint, the global `VoxelId` lookup, order independence | complete |
| M8E | traversal integration: plan-aware world vegetation, keep-out from the real body radius, `SurfaceGrid`, audit, route, enemy host, gate passage, continuous route | complete |
| M8F | presentation and evidence: the client visibility oracle, real rendering, captures, motion, weather, performance | complete |
| M8G | technical self-QA: full regression, the real game, fresh captures, documentation. Then stop for the owner playtest | complete |
| QA | independent branch QA over `main...HEAD`: code review, adversarial oracles from the drawn voxels, runtime gate passage, re-derived audit and route, driven visual and motion evidence, regressions, gates, documentation reconciliation | complete |

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

## What the branch implements

| part | where | what it is |
|---|---|---|
| materials | `crates/procedural/src/landmark/material.rs` | `LandmarkMaterial::{Stone, Band, Cap}` over the declared `VoxelId` range `224..256`, with albedo, specular and the palette rules of `LANDMARK_STYLE.md` |
| descriptor | `landmark/descriptor.rs` | `MonolithDescriptor`: class, height, widths, opening, lintel, rise, break, rubble, band period, lean, span axis. Drawn from a seed, canonicalised, validated, fingerprinted |
| compiler | `landmark/compile.rs` | descriptor to voxels: `CompiledMonolith`, per-column spans, top-down erosion, the `Silhouette` measurements the style rules are checked against |
| world proxy | `landmark/visibility.rs` | `WorldOccluders` and `VisibleBand`: how much silhouette a point in the world sees past terrain, water and canopy. No camera, no pixels, no fog |
| plan | `landmark/plan.rs` | `LandmarkPlan::derive`: the overlook, three sites, three instances, the vegetation reservations, and typed failure when the world cannot carry the composition |
| world | `procedural/src/generator.rs` | the plan is derived when the world is built; `generate` writes landmark voxels into air after vegetation; `WorldVegetation` is the one composed answer about plants |
| keep-out | `apps/client/src/traversal.rs` | the widest body's capsule turned into a `TraversalLegality` veto and a `SurfaceGrid` flag |
| presentation oracle | `apps/client/src/landmark.rs` | the real camera, the real projection and the real fog, used to check what the placement claims |
| renderer | `apps/client/src/renderer.rs` | one ordered lookup: terrain table, then landmark table, then the M3 diagnostic palette |

## The composition the golden world produced

`LANDMARK_BEHAVIOR_SIGNATURE = 0xe97b_ee8f_8573_7e95`, which is the plan's own fingerprint.

| | class | column | from the overlook | base → top | footprint | voxels | silhouette seen | sky |
|---|---|---|---|---|---|---|---|---|
| first choice | Spire | `(-130, 62)` | 62 u | 18 → 45 | 7 × 7 | 716 (465 stone, 202 band, 49 cap) | 21.3 voxels, 17.1 of them under 12° | yes |
| first choice | Gate | `(-7, 58)` | 63 u | 19 → 46 | 3 × 13 | 547 (270, 259, 18) | 22.0 voxels, 16.9 under 12° | yes |
| revealed | Broken | `(142, 71)` | 212 u | 18 → 40 | 4 × 17 | 466 (304, 130, 32) | 22.0 voxels from the gate at 149 u, all under 12° | yes |

The overlook resolved to `(-69, 49)`, ground face `19`, clearing radius `40` — the same column M5 stood its portrait in, M6 fought in and M7 started its route from, now derived from the world's own composition rather than chosen by the client. The two first choices stand at bearings `258.0` and `98.3` degrees from it, `159.7` apart — the probe prints the three numbers, so it is a measurement rather than a claim — which is the concrete meaning of "two directions that mean different things": the player cannot see both without turning around.

The gate's opening is `21` columns of ground with stone beginning `23` voxels up.

## Evidence

### The plan is a function of the world

`the_plan_is_a_pure_function_of_the_world` derives twice and compares fingerprints, origins, base courses, crown columns and compiled geometry. `terrain-probe landmarks` re-derives at runtime and refuses to print if the second plan disagrees with the one the world was built with. Chunk order independence is checked by `a_landmark_crossing_a_seam_is_written_identically_from_both_sides` and by M4's own order test.

### Derivation cost, measured on the audited host

| profile | cost of `LandmarkPlan::derive` for the golden world |
|---|---|
| release | **248–409 ms** over two sessions on the audited host |
| debug | **1.1–1.5 s** |

The first session recorded `248`–`272` ms and `1,108` ms; branch QA re-measured `316`–`409` ms and `1,486` ms on the same machine with other work idle. The spread is host load rather than a change in the work, and it is an observation on one host rather than a target — no test asserts it.

The owner's instruction was to measure before choosing a reuse strategy. The measurement says two things. First, `cargo nextest` runs **one process per test**, so no in-process cache — `OnceLock` or otherwise — can help the suite at all; the only levers are the derivation's own cost and the number of tests that build a world. Second, the reuse that does matter is two worlds of the same identity inside one process, which the client had three of: `World::source()`, `World::generator()` and `World::fingerprint()` each built one. `WorldSelection::build` now derives once and shares the plan behind its `Arc`, and `TerrainGenerator::with_plan` is the explicit way to say so. No hidden cache, no lazy first call.

Suite cost, `cargo nextest run`, this host:

| crate | before M8 | after M8 |
|---|---|---|
| `veldwake-procedural` | 60 s, 69 tests | 94 s, 97 tests |
| `veldwake-client` | 114 s, 171 tests | 142 s, 178 tests |
| workspace | 84 s, 699 tests | 746 tests, all passing |

Five optimisations took the derivation from `3.2 s` to `248 ms` in release: a five-point pre-filter before the full level-and-dry disc, a cached water lookup, `with_reservations` that keeps the terrain cache when the world's reservations change, solving the visible band in one pass instead of a nineteen-step binary search, and a straight-line dry test that prunes wet sites before any sight line is cast.

### The world after landmarks, from the reachability audit

Same audit as M7, re-run on the branch. Everything not listed is identical.

| measure | M7 | M8 | reading |
|---|---|---|---|
| standable columns | 622,023 | **621,798** | the keep-out removes 225 columns |
| reachable from the overlook | 307,162 | 306,937 | the same 225 |
| bidirectional | 307,144 | 306,919 | the same 225 |
| symmetric components | `[314861, 307144, 16, 2]` | `[314861, 306919, 16, 2]` | **no landmark cuts anything off** |
| barrier: traversal | 2,464 | **2,840** | +376 steps now refused by stone rather than water |
| barrier: step-up | 44,495 | 44,495 | unchanged |
| barrier: drop | 1,273 | 1,273 | unchanged |
| highland reachable | 243,333 | 243,333 | unchanged |

Every landmark can be walked to: the nearest standable column is 4 columns from the spire at **65 steps**, 1 column from the gate at **61 steps**, and 1 column from the broken monolith at **211 steps**.

### The fight is at a landmark

`fight_host` picks the first choice that does **not** reveal the third, so one direction ends in a reveal and the other in a fight — the two directions differ in what they mean, not only in what they look like. In the golden world that is the spire, and `place_adversary` with `PlacementRules::hosted_by` puts the adversary at `(-135, 62)`: five columns west of the crown, `69` steps from the overlook, first candidate examined of 475 level ones, level for seven columns, clear of vegetation for seven, dry and reachable both ways.

The route therefore moved with it: `(-69, 49) → (-135, 62)`, **70 columns, 5 waypoints, 97.17 units, 28.6 s at 3.40 u/s**, crossing two chunk boundaries on each axis, against M7's 163 columns and 217.09 units.

### A landmark is solid, and a gate is not

`BODY_KEEP_OUT_RADIUS = 0.86` and `BODY_KEEP_OUT_HEIGHT = 2.84` are the widest and tallest body the world carries — the adversary's capsule, radius `0.8274`, height `2.8303` — measured from the compiled rigs by `the_keep_out_is_the_widest_body_the_world_carries` rather than copied. Neither number exists in `veldwake-procedural`.

- `a_body_cannot_walk_into_a_landmark`: the runtime veto, the cached grid and `standable` all refuse a grounded column of every landmark.
- `a_landmark_blocks_the_runtime_rule_and_not_only_the_audit`: `check_move` answers `MoveBlockReason::Traversal` for a step into stone, the same answer the river gets.
- `the_gate_is_a_gate_and_a_body_can_walk_through_it`: a breadth-first walk from one side of the gate to the other, over the real `SurfaceGrid` with the real `MovementSpec`, crosses **through the footprint**, and every column it crosses has stone at least `BODY_KEEP_OUT_HEIGHT` above the floor.
- `the_runtime_walks_through_the_gate_and_not_only_the_audit` takes that crossing and puts every walk-speed increment of it to `check_move` with the production `TerrainGround` and `TerrainWalkability`, because a column graph is an upper bound and the owner's condition was about the body, not the lattice. None of the steps is refused.
- `the_cached_grid_agrees_with_the_runtime_veto_around_every_landmark` compares 771 columns around the three landmarks; the M7 agreement test still compares its 6,144 water columns.

`GroundSampler` is untouched. Nothing walks on a landmark: the keep-out is a refusal, never a surface.

### What it looks like

Five captures on the audited host, `1920 x 991`, release client, `m4-golden` profile, exit code `0`.

| capture | what it shows |
|---|---|
| the spire from the overlook, clear | the monolith stands clear of the canopy, banded, darker than the valley wall behind it, unmistakably made |
| the gate from the overlook, clear | two banded shafts and the sky between them, read as one object at 63 units |
| the gate at ten units | the opening reads as a passage: ground continues through it, and the shafts carry the style's value break |
| the broken monolith from the gate | framed between the gate's own shafts at 149 units, a pale vertical against the tree line |
| the spire, overcast | the silhouette survives the second weather state: lower contrast, same read |
| **the real session**, `VELDWAKE_ENCOUNTER=traverse` | the shot the milestone is about: the player's body in the overlook clearing with its sword and health readout, and the spire standing over the tree line ahead of it. Nothing in the frame points at the spire; the spire is the thing that points |

The session the owner will run was started too, on the audited host, and it reports what it should: `encounter ready mode="traverse"` with `player_start=[-68.5, 49.5]` and `adversary_start=[-134.5, 62.5]` — the adversary at the spire — followed by `traversal route ready rule_version=2 start=(-69, 49) goal=(-135, 62) columns=70 waypoints=5 length_units=97.17 walk_seconds=28.58 signature=0xa06c9d72a8824848 locked=0xa06c9d72a8824848 matches_locked=true reachable_columns=306937 standable_columns=621798 highland_reachable=true`, its four checkpoints, and an audio device open with no errors. No panic, no validation error, exit `0`.

The capture at the overlook reported `59.9` FPS, `render = 2,197` demanded against `presented = 334` and `gpu_quads = 343,812`, with `gaps_closed`, `upload_failures` and `commit_invariant_failures` all zero — and it had **not** reached idle coverage twenty-six seconds in, which is KI-017 unchanged. The landmarks are drawn regardless, because at 62 and 63 units they are inside the first chunks to arrive. That is worth knowing for the playtest: the far valley fills in for about a minute, and the things this milestone is about are there from the start.

**The crown of a first choice sits above the default camera frame.** At 62 units the spire's top is `19.9` degrees above the camera and the frame's top edge is `13.75`, so a player at rest sees a tower leaving the top of the screen and looks up to see its cap. This was measured, then tested against the alternative: requiring the whole silhouette inside the frame pushes the pair out to 87 and 96 units, where the canopy leaves **8.3 and 11.5 visible voxels instead of 21.3 and 22.0**. Both compositions were captured before choosing. The near one is a tower; the far one is a sliver. `the_crown_of_a_first_choice_stands_above_the_default_frame` locks the choice so the next agent reads it as a decision.

## Accepted behaviour

- A first choice is framed from the overlook but not framed *entirely*: the crown is above the default pitch until the player looks up or walks closer.
- The revealed landmark is exempt from the crown rule the first pair is not required to satisfy either, and for a measured reason: of 111 candidate sites far enough from the first two to be a third landmark, the forest leaves **2** visible from a landmark at all, and neither is far enough for its crown to clear the frame.
- A diagonal step of the golden route brushes one corner of the spire's keep-out. The runtime accepts it — legality is destination-only under ADR-0008, and `the_runtime_accepts_every_step_of_the_route_it_walks_continuously` walks it — and the corner test now counts brushes rather than forbidding them, because its own name says "between **two** blocked columns" and one blocked shoulder is a body passing an obstacle.
- A diagnostic world under an arbitrary seed may not carry the golden composition. Two of five seeds tried do; the other three fail with a named reason and are built **without landmarks** rather than with a landmark placed where the composition does not hold. Only the golden world is required to carry all three, and a test says so.

## Locks moved in this branch

| lock | old | new | why |
|---|---|---|---|
| `LANDMARK_BEHAVIOR_SIGNATURE` | none | `0xe97b_ee8f_8573_7e95` | first lock; the golden plan's own fingerprint |
| `TERRAIN_BEHAVIOR_SIGNATURE` | `0x2d88_497f_a6d6_d4b5` | `0x2d59_3291_f052_9f23` | landmark voxels and suppressed plants change what the world generates |
| `GOLDEN_WORLD_BEHAVIOR_SIGNATURE` | `0x2d88_497f_a6d6_d4b5` | `0x2d59_3291_f052_9f23` | the same value, exhaustively re-derived |
| `GOLDEN_REGION_SIGNATURE` | `0x1285_7799_1516_4f6a` | `0x0cd9_5da6_1656_b11b` | two fixture chunks hold part of a landmark |
| `TRAVERSAL_RULE_VERSION` | `1` | `2` | a landmark is a new reason to refuse a destination |
| `ADVERSARY_COLUMN` | `(14, 191)` | `(-135, 62)` | the fight is hosted by a landmark |
| `GOLDEN_ROUTE_SIGNATURE` | `0x08c1_0aea_5280_b90f` | `0xa06c_9d72_a882_4848` | world identity, rule version and destination all moved |

`TERRAIN_GENERATOR_VERSION` is deliberately **not** bumped: `WorldSeed::stream` folds it into every stream, so bumping it would reseed the valley and regenerate a world nobody asked to change. M5's and M6's domain signatures are byte-identical; the character, the weapon, the encounter and the audio were not touched.

## Gates

| gate | result |
|---|---|
| `cargo fmt --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo build --workspace --all-features` | PASS |
| `cargo nextest run --workspace` | PASS — `746 tests run: 746 passed, 3 skipped` |
| `cargo nextest run --workspace --run-ignored all` | PASS — `749 tests run: 749 passed, 0 skipped`, so every `#[ignore]`d fixture was run deliberately |
| `cargo test --workspace --doc` | PASS |
| `cargo metadata --format-version 1 --no-deps` | PASS |
| `cargo deny check` | PASS — advisories, bans, licenses, sources |
| `cargo audit` | PASS — no vulnerabilities in 226 dependencies |
| driven capture smoke on the audited host | PASS — eight driven release runs, every capture `1920 x 991` and every run exit `0`; the five read in writing are in the table above |
| owner discovery playtest, without navigation instructions | **PASS** — 2026-09-22, see the owner section below |
| independent branch QA | **PASS** — see the branch QA section below |

## OWNER DISCOVERY PLAYTEST WITHOUT NAVIGATION INSTRUCTIONS: PASS

2026-09-22, by the repository owner. **The wording is exact on purpose.** During the session the owner received no coordinate, no direction, no waypoint, no route and no hint about which landmark hosts the adversary; what the owner did with the world was entirely unprompted. The owner had, however, read the implementation report before playing, so this was not a session with zero prior knowledge, and calling it *blind* would claim more than happened. The discovery itself is unaffected: nothing told the owner where to look, and the owner looked, chose and walked.

**What the owner observed, in the owner's own terms:**

- noticed **two** structures nearby, without any navigation interface;
- read them as **two different destinations**;
- read one of them as **a tower**;
- read the other as **a broken construction, an abandoned ruin**;
- **chose deliberately** to go to the tower;
- **found the adversary while exploring**;
- liked the result;
- judged the structures **simple, but good and sufficient for the current objective**;
- observed that they still read somewhat close to the visual language of Cube World and familiar voxel RPGs.

**This closes the milestone's core product gate.** The hypothesis M8 existed to test — that directions in this world now mean different enough things to provoke a spatial choice — is the thing the owner's session demonstrates, and it demonstrates it in the only way that counts: a person who was told nothing looked at the world, saw two things, decided between them, and went.

### The product change, stated plainly

| | what a person did |
|---|---|
| **M7** | walking the world works, and the owner found nothing in it |
| **M8** | the owner immediately noticed two different destinations, chose one, explored toward it, and found the adversary |

### What the owner did *not* judge

Everything else in this document is technical or self-QA evidence and stays that way. The owner gave no measurement of distance, no formal contrast judgement, no judgement of the third landmark, no systematic clear-versus-overcast comparison, no judgement of whether the gate can be walked through, and no judgement of the cropped crown recorded as KI-032. Nothing in this repository may attribute any of those to the owner.

### Two observations to carry forward, neither of them a blocker

**"The landmarks still read as visually simple, but are acceptable for the M8 discovery slice."** Recorded as the owner's words and deliberately not answered with work: no props, no loot, no interiors, no decoration, no second material system, no second family, no particles, no banners, no lights, no extra ruins. M8 asked one question and the question is answered.

**The family can still read somewhat close to the visual language of Cube World and familiar voxel-RPG architecture.** Not an M8 defect and not something to correct on this branch. It is evidence for a future decision about architectural art direction, a stronger Veldwake-specific shape grammar, and a procedural built-world identity — recorded in [`../KNOWN_ISSUES.md`](../KNOWN_ISSUES.md) as KI-036 alongside the other product and art limitations that milestone kept rather than hid.

**One reading is worth separating from both of those, because it is about this composition rather than about the art direction.** The two landmarks visible from the overlook are the Spire and the **Gate**, and the owner read the second one as a broken construction. The style contract's one-sentence test asks that a person can name the broad gate as a gate; the distinctness half of that test passed and the naming half did not. What this agent can add is a mechanism, not a judgement, and it is an inference rather than anything the owner said: from the overlook the gate's lintel is above the frame (KI-032), so what a viewer sees at rest is two shafts and the sky between them — which is what a fallen frame looks like. The owner was not asked and did not comment on it.

## Independent branch QA

A separate review of the whole of `main...HEAD`, run after the owner's session and before any pull request. Its job was to try to break what the implementation claims, using oracles that do not ask the implementation about itself: the drawn voxels, the authoritative tick loop, a landmark-free baseline world, and the real client.

### One defect, found and fixed

**`CompiledMonolith::new` accepted a descriptor it could not compile.** The constructor was documented "infallible by construction" and relied on every caller using `MonolithDescriptor::from_seed`, which the type system did not require. With a hand-built descriptor the compiler read `band_period` as a divisor and panicked on `0`, and read negative dimensions as lengths and produced an **empty** landmark, which a plan would then have placed in the world as a landmark with no voxels. Neither is reachable from the golden path — `from_seed` draws `band_period` from `3..=4` — and the crate already had the right answer in `MonolithDescriptor::validate`; the constructor simply never called it.

The fix is the smallest one that makes the documentation true by type: `CompiledMonolith::new` returns `Result<Self, DescriptorError>`, `PlanError` gained a `Descriptor` variant so a world that draws an uncompilable descriptor fails to be built rather than being built wrong, and `the_compiler_refuses_a_descriptor_it_would_have_to_guess_at` pins both directions — the guard rejects the three adversarial shapes, and it rejects nothing the draw produces across all three classes, both axes and sixty-four seeds.

### What the oracles proved

| question | oracle | result |
|---|---|---|
| does a landmark ever replace terrain or water? | every chunk a landmark touches, generated twice — once in the golden world and once in a world of the same identity built with `LandmarkPlan::bare` — and every differing voxel inspected | **no.** `524,288` voxels compared: `1,740` landmark voxels written, all over air or over a plant the landmark's own reservation had already removed, and `2,543` plant voxels removed. Not one terrain or water voxel moved |
| do the chunks depend on the order they are asked for? | six coprime-stride permutations, one world per chunk in isolation, and a clone | **no.** Byte-identical fingerprints in every order |
| does the keep-out agree with the voxels a viewer sees? | the landmark solids read out of the generated chunks, against `TerrainWalkability` at quarter-column resolution, including negative coordinates | **yes, in both directions.** No walkable position holds drawn stone, and the ground around each landmark is not fenced off |
| can a body really walk through the gate? | a real `Encounter`, real ground, real veto, real capsule, driven tick by tick with a walk intent | **yes through the opening** — it enters the footprint and arrives within `1.0` of the far side — and **no through a pillar**, where it stops more than `1.0` short |
| is a plant the grammar proposes inside a reservation really gone? | the raw grammar against the composed view, and the trunk column read out of the generated chunk | **gone at all three levels** |
| does a landmark stand on a shoreline? | every column within the `14`-column margin the plan samples on a two-column stride | **none.** The stride's approximation holds exactly here |
| is the overlook a place the finished world lets a body stand? | the field, the plan and the composed vegetation, asked about the resolved column | dry, level to a voxel for four columns, no landmark on it, no plant in the body's own band |
| does the world fingerprint react to the landmark controls? | thirteen controls changed one at a time | **all thirteen move it.** The identity's own "reacts to every input" test had not covered the field M8 added; it does now |

### What re-derivation confirmed

Every number the sections above claim was re-derived rather than trusted. The audit reproduces exactly: `621,798` standable, `306,937` forward, `307,334` reverse, `306,919` bidirectional, `18` one-way, components `[314861, 306919, 16, 2]`, barriers `2,840` traversal, `44,495` step-up, `1,273` drop, highland `493,837` standable and `243,333` reachable with the nearest `22` steps away. The approaches reproduce: `65`, `61` and `211` steps. The host and placement reproduce: the spire, `(-135, 62)`, `69` steps, first of `475` level candidates. The route reproduces: `70` columns, `5` waypoints, `97.17` units, `28.6` s, two chunk crossings on each axis, signature `0xa06c9d72a8824848` equal to the lock. `terrain-probe signature` reports `0x0cd95da61656b11b`, the relocked `GOLDEN_REGION_SIGNATURE`, without any test in the loop.

KI-034 reproduces precisely: the route takes `68` diagonal steps, exactly **one** of which passes a blocked shoulder, and **none** with both shoulders blocked. The continuous runtime test accepts every walk-speed sample of the route, so the brush never crosses stone.

### What the real client showed

Eleven driven runs on the audited host per [`../agents/EVIDENCE_HARNESS.md`](../agents/EVIDENCE_HARNESS.md), one process at a time, focus verified before every frame, client area `1920 x 991`, exit `0` and no error, panic or validation marker in any of them. Every capture was opened and read.

- **The discovery itself.** From the session start the spire stands clear of the tree line ahead of the body; turning the camera about `160` degrees brings the gate into frame in the opposite direction. Two destinations, no interface.
- **The gate, walked.** Approached from the overlook, the body meets a **pillar** and is held by it — it slides along the face and does not pass. Lined up with the opening, the same walk carries it **between the two shafts** and out the far side, with the ground continuing underneath.
- **The reveal.** From inside and beyond the gate, the broken monolith is a pale vertical above the tree line at `149` units, framed by the gate's own shafts.
- **The spire, close.** The foundation meets the grass with no gap and no sinking, the banding reads, and the body walks past it without entering it.
- **The adversary.** Reached by walking to the spire: `adversary_dormant` went to `false` and the player's health from `96` to `78`, with both health readouts drawn.
- **Weather.** Under overcast the gate still reads as two banded shafts against a pale sky.
- **Regressions by eye.** The M3 diagnostic corridor renders exactly as before — the renderer's new ordered material lookup did not touch the diagnostic palette — and a seeded diagnostic world draws its own composed landmark.

Streaming during a walking traversal, from the same runs: `render_radius = 6` and `Lod0Only` unchanged, `59.98` FPS at `16.67` ms average wall frame, `2,197` demanded, `507` presented, `501,922` GPU quads, `cpu_evictions = 66` (so the body really moved), and `gaps_closed`, `ready_undrawn_max`, `upload_failures`, `commit_invariant_failures` and `hard_cap_blocks` all zero.

### What QA corrected in this document

- **The derivation is slower than first recorded.** Re-measured on the same host: `316`–`409` ms in release over eight runs and `1,486` ms in debug, against the `248`–`272` ms and `1,108` ms first written down. Nothing about the work changed; the first numbers were taken in a quieter moment and were quoted as a range they do not cover. The tables above and KI-035 now carry the wider observation, and no test asserts a duration.
- **The three landmarks write `1,740` voxels, not `1,729`.** The smaller number is what the three compiled objects hold; the world also writes `11` voxels of foundation between the base course and the terrain under it.
- **The diagnostic-seed sample was too small to generalise.** KI-033 was written from five seeds; twenty seeds and five edge values give `14` composed, `6` bare and **zero** panics, including `0`, `u64::MAX` and the golden seed's neighbours. The contract holds and the rate is better than the first sample suggested.

### What QA observed and did not act on

- **The camera can end up inside a landmark.** Walking close past a shaft puts the follow camera inside the stone for a moment; the body and the way ahead stay visible and navigation is unaffected. This is [KI-025](../KNOWN_ISSUES.md) — no camera occlusion solving — meeting its first man-made occluder, and it is recorded rather than fixed, because a camera collision system is a milestone of its own.
- **A blind driver curves.** Holding "forward" for a hundred units in the driven harness lands about `11` degrees off, because the follow camera re-derives its yaw from the camera-to-anchor vector every frame and the lateral offset slowly rotates it. A player corrects continuously and never notices; a script does not, which is why three of the eleven runs missed the adversary's `14`-unit aggro radius entirely. It is a property of the harness plus the camera, not a defect in either, and it is exactly the effect M7 recorded when a blind driver could not aim.
- **The world proxy looks at trees, not at all vegetation.** `WorldOccluders::occluder` takes the canopy over a column and ignores shrubs, which the module's own documentation described as "vegetation". A shrub is one or two voxels and can at most hide a landmark's lowest course from an eye three and a half voxels up, so the proxy is slightly generous about a base and never about a crown — and the composition's margins are twenty visible voxels against a minimum of eight. The scope is now written down exactly rather than corrected, because correcting it would move the plan for no gain.
- **KI-036 stands untouched.** No prop, decoration, second family, second material, particle, light or interior was added.

## What this agent could not verify

Written before the playtest and kept as written, with what the owner's session did and did not settle.

- Whether the composition **works**. **Settled by the owner**: two destinations were noticed, read as different, and one was chosen and walked to.
- Whether the overlook clearing reads as artificial. **Still unsettled.** The owner did not comment on the clearing, and a capture taken from inside it cannot answer that honestly.
- Motion evidence beyond the at-rest captures: the harness's injected mouse-look did not reach the client in a static camera pose, so the "look up at the crown" frame is a measurement — `19.9` degrees against a frame edge at `13.75` — rather than a photograph.
- Whether the two directions read as *different* rather than as two of the same thing. **Settled by the owner**, who read one as a tower and the other as a broken construction and chose between them — though the second reading is not the class the compiler built, which the owner section records.

## Non-goals

Recorded here once so the rest of the document can refer to them: everything in the owner's out-of-scope row above, and in particular no walking on any landmark surface (roofs, lintels, rubble tops), no terrain excavation, no landmark material replacing a terrain voxel, and no change to `GroundSampler`, `check_move`, `MoveBlockReason`, the step and drop bounds, MOVE-001, combat tuning, the AI, the weapon, health, damage or the telegraph.
