# M7 — Traversable Region

Status: **implementation complete on `feat/m7-traversable-region`; awaiting OWNER PLAYTEST**
Base: `docs/post-m6-handoff` at `aac3ec55519d93e44397af549eb18089c7dcdfcd`, which is `main` at merge commit `f840ff7880e1857e86b3a74c4d3f66ceaf82a922` plus two documentation commits.

M4 proved the region can look like something. M5 proved a person can stand in it. M6 proved a person can fight in it — inside a disc of `5.5` world units, at one scanned clearing. M7 has to prove that the person can *leave that disc*: exist and move continuously through the real procedural region, with streaming anchored on the body, reach an encounter placed in the world, fight, and continue the session.

This document is written as the milestone lands, phase by phase. Anything not yet measured is not claimed.

## Scope, as accepted by the owner

> Provar que o personagem controlado pelo jogador pode existir e se deslocar continuamente pela região procedural real, com streaming dirigido pela posição do corpo, chegar a um encontro colocado no mundo, lutar, e continuar a sessão.

M7 is **not** persistence, **not** world simulation, and **not** content expansion. It is a vertical slice of world traversal.

The accepted scope has six items:

1. A player-controlled body outside combat.
2. Continuous movement over the real procedural region using the existing kinematic rules.
3. Streaming demand anchored on the body during gameplay.
4. One adversary placed in the world and dormant until a proximity condition.
5. One deterministic named route that can be analysed headlessly, walked in the client, used for evidence, and that ends in a real encounter.
6. The session continues after the fight.

## Phases

| phase | contents | status |
|---|---|---|
| M7A | movement/domain correctness: exact support height, `MoveBlockReason`, `TraversalLegality`, optional arena, explicit initial positions, player-victory policy, `WorldContact` | complete |
| M7B | canonical voxel-water predicate, `TerrainWalkability`, TRAVERSE-001, reachability audit, named route, route signature, runtime route proof, enemy placement | complete |
| M7C | playable traversal client: `VELDWAKE_ENCOUNTER=traverse`, body-anchored streaming, dormancy reporting, outcome policies | complete |
| M7D | measurement and visual/motion QA on the audited host | complete |
| M7E | documentation and exit evidence | complete except the owner playtest |

## Owner decisions

Recorded verbatim in substance, because they are the constraints the milestone answers to and they are not re-derivable from the code.

| decision | ruling |
|---|---|
| milestone name | **M7 — Traversable Region**. "Inhabited Region" was accepted conceptually but rejected as a name: no ecosystem or society inhabits the region yet |
| owner playtest | **required as an exit gate.** The M6 owner playtest does **not** transfer. No agent can close this gate |
| water | **water blocks traversal.** No swimming, wading, water movement, stamina or boats |
| `GroundSampler` | **semantics must not change.** It answers "what is the visible solid support surface"; in a water column it returns the bed. That stays correct for feet, IK, pelvis, pose and exact support height |
| reachability vs route | **two separate things.** The audit measures connectivity and may legitimately discover unreachable regions — that is evidence, not automatically a bug. Separately, one named route must be genuinely completable under the accepted rules |
| jump / climb / swim | **must not be added to make the audit look better** |
| running | **not pre-approved.** Route length and measured duration are information, not permission. Adding a second speed requires measured dead time and a fresh proposal |
| KI-017 | **do not fix in this round.** Measure. Only a measured coverage failure authorises the smallest correction |
| renderer | **no frustum culling, batching or instancing overhaul** without a real finding |
| KI-025 | remains open; no occlusion solver |
| M6 owner gates | not re-evaluated; synth, action curves, telegraph and combat feel are not touched |

## Mandatory design corrections

The technical design was approved **with corrections**. They are recorded here because each one closes a real architectural defect, and because the reason matters more than the change.

### 1. A smoothed pelvis is not an authoritative height

`CharacterState::base_height` is documented in `pose.rs` as the **smoothed** height the character stands on: the feet snap to the exact visible block top, the pelvis follows a filtered height, and leg IK absorbs the difference. That is a presentation-facing filter.

`movement::accepts` compared candidate ground against that smoothed value, which couples presentation smoothing to movement authority. Movement legality must compare the **exact support surface under the body** against the **exact support surface at the destination**:

```text
source_surface = ground.surface(current_x, current_z)
target_surface = ground.surface(target_x, target_z)
step_up        = target_surface - source_surface
drop           = source_surface - target_surface
```

The pelvis stays smoothed, for pose only. M6 happens on flat ground, so every M6 signature must remain byte-identical.

### 2. One rule, one place, one reason

The audit needs to attribute a refusal to a cause. Recomputing "why did that fail" outside the rule would be a second implementation of the rule. A single typed outcome — `MoveBlockReason` — is produced by one central function, and `accepts` becomes a wrapper over it. `try_move` and the audit use the same function.

### 3. World placement is not combat tuning

Where in the world a body starts is not attack, movement or health tuning. M7 needs two explicit initial positions, because the player starts at the route start and the adversary at the route goal, far apart. One `origin` plus a large offset would be obscure, and would make a single value mean arena centre, enemy site, player respawn and reset point at once.

### 4. The outcome policy names the side it is about

The new case is specifically what happens when the **adversary** is defeated. A policy named for "victory" in general could read as applying to both sides. M6 resets; M7 leaves the defeated adversary where it fell. Player defeat keeps the M6 defeat hold and reset, to configured initial positions.

### 5. One definition of "this column has water"

The generator writes a water voxel exactly when `water_surface_y() > surface_y()`. That predicate belongs in the procedural domain, once, so generator tests, the traversal veto and TRAVERSE-001 all speak about the same property. `TerrainSample::is_submerged()` keeps meaning the **continuous** relation `water_surface > height`, and the two genuinely disagree on some quantized columns.

### 6. Region bounds are interpreted once

`TerrainGround` already carries the correct half-open interpretation of the region extent. A second bounds arithmetic that could drift is not acceptable.

### 7. Brain state is read inside the tick loop

The client gates a traversal dodge request on the adversary being dormant. That gate must be evaluated **inside** the tick loop, not hoisted above it: a frame can run four ticks, the first of which crosses the aggro radius, and the remaining three must observe the new brain state. Hoisting would make behaviour depend on the frame partition.

### 8. No invented time budget

The audit's cost is measured and recorded as an observation. A test must not fail on an arbitrary wall-clock threshold. Where the audit lands — normal test or probe/ignored with a CI subset — is decided by observed cost.

### 9. Streaming determinism is not over-promised

Authoritative: body position per tick, the route, enemy placement, the wake tick for a given input trace, the encounter. Not authoritative: frame-by-frame anchor samples, worker completion, upload timing, settle timing, camera, pixels. The anchor is sampled once per rendered frame, so no golden signature is made of the raw anchor sequence.

### 10. Traversal partition equivalence is proved, not inherited

"It uses the M6 clock" is an argument, not evidence. A new integration test delivers the same exact total elapsed time and the same input trace in 30/60/144 Hz partitions and compares the authoritative end state.

## Proposed invariant

**TRAVERSE-001 — water is what the viewer can see.** A column is untraversable by water exactly when the generator writes at least one water voxel in it. A traversal predicate that disagrees with the drawn blocks is wrong however elegant it is. This is CHAR-002 applied to traversal.

## Explicit non-goals

Persistence and saves; world editing; jump; climbing; swimming; wading; fall damage; stamina; mounts; boats; gliders; fast travel; dodge outside combat; world clock, day/night and seasons; a second region, biome, enemy or weapon; creature and ecology systems; inventory, loot, crafting, progression, quests, economy, settlements, history; UI framework, menu, pause, death screen; ECS, generalized entity model, spatial entity index; navmesh and pathfinding; physics engine; vegetation collision; camera occlusion solver; networking and multiplayer; WASM; music, ambience and mixer; speculative renderer optimization.


## M7A — the rules

### A step is judged between exact support surfaces

`movement::accepts` compared a candidate column's height against `CharacterState::base_height`, which `veldwake-character` documents as the **smoothed** height the pelvis follows. The feet snap to the exact visible block top; the pelvis lags; leg IK absorbs the difference. That filter is presentation, and movement authority was reading it.

The consequence was not hypothetical. At `PELVIS_RISE_TAU = 0.12 s` and `120` Hz the pelvis closes `6.71%` of the gap per tick, so a body climbing at gradient `g` at the walk speed carries a steady-state lag of about `0.42 g` world units. The test is `destination - source <= max_step_up` with `max_step_up = 1.0` and a terrain voxel of exactly `1.0`, so **any** positive lag turns a legal one-voxel step into an illegal one. The same step was legal standing still and illegal while walking, and how illegal depended on how long the body had been climbing.

Movement now compares the exact support surface under the body against the exact support surface at the destination, both from `GroundSampler::surface`. The pelvis stays smoothed, for pose only.

M6 never saw this because its arena is asserted **exactly level** out to seven columns and its headless fixtures stand on `FlatGround`; source and destination heights are equal there either way. Every M6 signature is byte-identical.

Eight regressions were added:

| test | what it holds |
|---|---|
| `a_rise_of_exactly_the_bound_is_walkable_and_an_epsilon_more_is_not` | `max_step_up` is inclusive; `+1e-4` is `StepUp` |
| `a_drop_of_exactly_the_bound_is_walkable_and_an_epsilon_more_is_not` | the same for `max_drop` |
| `a_step_is_judged_from_the_ground_under_the_body_and_never_from_the_pelvis` | three bodies on identical trajectories with the pelvis settled, half a unit behind and forty units behind reach bit-identical positions over six hundred moves |
| `a_body_climbs_consecutive_one_voxel_terraces_without_stalling` | a real staircase at the real walk speed: thirty-plus terraces, **zero** blocked moves, and the distance travelled matches the distance asked for |
| `a_real_encounter_walks_a_stepped_ramp_without_the_pelvis_stopping_it` | the same through the whole authoritative tick loop, with the pelvis filter running for real: zero stalls over 2,400 ticks |

### One rule, one place, one reason

`check_move` is now the only implementation and returns `Result<(), MoveBlockReason>`. `accepts` is a wrapper over it, `try_move` uses it, and the reachability audit calls it rather than owning a second copy of the conditions. The order of the checks is part of the contract because the audit attributes a barrier to the **first** cause that applies:

```text
1 NonFinite   2 Arena   3 Traversal   4 MissingGround (source, then destination)   5 StepUp   6 Drop
```

`every_block_reason_is_reachable_and_the_first_that_applies_is_reported` produces all six and asserts the precedence.

### Placement is not tuning

`AuthoredTuning` lost `arena_centre` and `arena_radius`. Where a body starts and what bounds it are `EncounterSetup`'s: `starts: [Vec2; 2]` in absolute world units, and `arena: Option<ArenaSpec>`. One `origin` plus two opposite offsets would have named a point that is neither body, neither the arena, nor anywhere a fight happens — which is exactly what a route start and a route goal a hundred and forty-two columns apart are.

### The outcome policy names the side it is about

`PlayerVictoryPolicy::{ResetEncounter, Remain}`, and `Resolution::{Fighting, Holding, Settled}` rather than an `Option<Side>` plus a flag. `Settled` exists because the M6 code re-derived the hold's arithmetic on every tick for ever once the hold expired; under `Remain` that would have been the rest of the session.

M6 fixtures keep `ResetEncounter`, which is why `GOLDEN_ENCOUNTER_SIGNATURE` did not move.

### `WorldContact`

`Encounter::step(intent, world)` where `WorldContact` carries a `GroundSampler` and a `TraversalLegality` as named fields, built by `terrain`, `ground_only`, `from_ground` or `none`. Two optional trait references of the same shape as positional parameters swap places silently; sixty-four call sites is exactly the number where that matters.

## M7B — the world

### One definition of "this column has water"

`TerrainSample::has_water_voxel()` lives in `veldwake-procedural` beside the generator that writes the voxels: `water_surface_y() > surface_y()`, which is precisely the condition under which `fill_terrain` writes at least one water cell. `is_submerged()` keeps meaning the continuous relation `water_surface > height`.

They genuinely disagree, and `the_continuous_and_the_voxel_water_predicates_genuinely_disagree` proves the disagreement set is non-empty over six named chunks rather than assuming it. A column at `height = 15.2` under a water surface of `15.8` is submerged by the continuous relation and has no water voxel at all: a viewer sees dry ground. Traversal follows the voxels.

**TRAVERSE-001** is that rule as an invariant, and `traversal_water_agrees_with_the_voxels_the_generator_writes` asserts it by generating the chunks and comparing every column against the drawn cells.

### Region bounds, interpreted once

`world::RegionBounds` is the half-open rectangle, shared by `TerrainGround` and `TerrainWalkability`. M5's public ground semantics are unchanged.

### The reachability audit

`SurfaceGrid::sample` takes one pass over the region's own chunk order and stores a support height, a water flag and a biome zone per column. The audit then walks it with **`veldwake_combat::check_move`** — the game's rule, called — over an eight-connected directed graph, because the client hands the rules a diagonal intent and `try_move` tests the whole diagonal before either axis.

The grid exists for cost alone, and `the_cached_grid_agrees_with_the_runtime_adapters` asserts it answers identically to `TerrainGround` and `TerrainWalkability` at every column centre of six chunks — 6,144 columns — because without that it would be a second, silently drifting copy of the world.

**The result, on the golden region.** These are the region's properties, not targets:

| measure | value |
|---|---|
| grid | 800 x 800 = 640,000 columns |
| columns with ground | 640,000 |
| columns a body could stand in (dry, in region) | 622,023 |
| reachable from the route start | 307,162 — **49.4%** of standable |
| from which the start is reachable | 307,445 |
| reachable both ways | 307,144 |
| reachable but not returnable — one-way drops | 18 |
| largest components of the symmetric relation | 314,861 · **307,144** · 16 · 2 |
| barrier: water | 2,464 |
| barrier: step-up | 44,495 |
| barrier: max-drop | 1,273 |
| barrier: non-finite / arena / missing-ground | 0 / 0 / 0 |
| highland columns standable | 493,837 |
| **highland columns reachable** | **243,333** |
| nearest reachable highland | `(-83, 27)`, 22 steps from the start |

**The highland is reachable, and the prediction that it would not be was wrong.** The design round computed the valley wall's steepest gradient as `1.875 x 48 / 52 = 1.73` voxels of rise per voxel of run and predicted that a body limited to a one-voxel step-up might not be able to leave the valley floor. It can, and from twenty-two steps away. The wall is steep at its steepest point and the region offers routes that are not.

**The region is split, and the start is in the smaller half.** The two large components are 314,861 and 307,144 columns, and the route start is in the second. Nearly half the standable region is a place a body can stand but cannot walk to from here. That is evidence rather than a defect, it is recorded rather than engineered around, and no jump, climb or swim was added to make the number look better.

**Cost, release, audited host:** grid sample `590` ms, audit `273` ms. Cheap enough that the whole-region audit runs in the ordinary test set. The **placement search** costs about `14` seconds, because it asks the vegetation grammar about a thousand candidate clearings, so it is run deliberately by `the_derived_placement_is_the_locked_one` and every other test reads the locked result. That split is a cost observation; no test asserts a duration.

### Where the adversary stands

Derived, then locked. Candidates are columns reachable from the start, at least `90` steps away, dry, and level for seven columns; the survivors are ordered by how close their step distance is to `150` and then by `(z, x)`, and the first that is also free of vegetation for seven columns and sixteen voxels up wins. 18,303 columns passed the cheap filters and 1,312 were examined before one passed the vegetation check.

`ADVERSARY_COLUMN = (14, 191)`, `162` steps from the start.

### The named route

Derived from the audit's own breadth-first tree, so it is the shortest walk the rules allow and no waypoint was chosen by hand.

| measure | value |
|---|---|
| start → goal | `(-69, 49)` → `(14, 191)` |
| columns | 163 |
| waypoints after collapsing collinear runs | 40 |
| length | `217.09` world units |
| walking duration at `3.40` u/s | `63.9` s |
| chunk boundaries crossed | 3 on `x`, 4 on `z` |
| `GOLDEN_ROUTE_SIGNATURE` | `0x08c1_0aea_5280_b90f` |

Checkpoints: `route-start`, `first-chunk-crossing` at `(-64, 54)`, `steepest-leg` at `(-45, 73)`, `route-goal`. The `water-edge` checkpoint is defined but was not emitted, and its absence is informative: **the shortest legal route never comes within six columns of water.** The rules route around the river without being told to.

### The proof that matters

`the_route_is_walkable_in_a_real_encounter` drives a real `Encounter` tick by tick over the real terrain adapters, following the waypoints by intent. It asserts the body arrives, never enters water, never leaves the region, stays finite, never exceeds the walk speed per tick while walking, and that the adversary wakes. It passes.

The settled-column audit and the runtime agree. That agreement is the thing being tested, and it is why the audit is described as a topological upper bound everywhere it is described at all.

## M7C — the session

`VELDWAKE_ENCOUNTER=traverse`. The player starts at the route start, the adversary at the derived goal, there is no arena, water blocks, and beating the adversary leaves it where it fell.

**The encounter is persistent and the entity model is unchanged.** There is no `PlayerState`, no handoff and no entity model: the player is `Combatant[Player]` for the whole session, which is the state a traversing body needs and nothing more. ADR-0005's review trigger is a third actor, and there are still exactly two.

**Dormancy already existed.** `AdversaryState::Idle` returns zero intent, draws nothing from its deterministic stream, and transitions at `aggro_radius = 14.0`. `a_distant_adversary_is_dormant_and_costs_the_stream_nothing` holds a body 200 units away for 10,000 ticks and asserts bit-identical position and facing, an unchanged decision counter, and zero events.

One small finding came out of writing that test: `start_state` hands out an unwrapped facing and the first `turn_toward` normalises `pi` to `-pi`. A representation moves; a body does not. The test steps one tick before it starts watching and says why.

**The dodge gate is read inside the tick loop.** A traversal dodge is an M7 non-goal, so the client does not ask for one while the adversary is dormant — a gate on intent, which the client may set, rather than a new rule in the domain, which it may not. `the_dodge_gate_is_read_inside_the_tick_loop_not_above_it` walks the body up the derived route to within one frame of the aggro boundary, fires a four-tick frame with a dodge pressed, and asserts the press is judged exactly once and the frame wakes the adversary. Hoisting the brain-state read above the loop would make a dodge depend on how the frames were cut.

**Streaming follows the body.** `track_camera` became `track_anchor` because the function was always generic and only the name was not. A traversal session anchors on the player's stand point at the start of the frame — the same value the camera was aimed at, `0.057` world units of lag at the walk speed against a chunk edge of `32` — and `F4` detaching the camera does not change that. Free-fly and every M6 mode still anchor on the camera. The report says which.

## M7D — measurement, on the audited host

Release, Intel Core i5-1335U / Intel Iris Xe / D3D12, `m4-golden` profile, golden seed. Observations on one host, never budgets.

### Streaming under a walking body — the KI-017 question

A `239`-unit walk at `3.40` u/s in seventy seconds, no captures taken:

| measure | value |
|---|---|
| loads dispatched, per five-second interval | 125–208 (**25–42 per second**) |
| the rail's ceiling: one job per `poll`, one `poll` per frame at 60 Hz | ~60 per second |
| `gaps_closed` / `gap_frames_total` / `gap_max_simultaneous` | 0 / 0 / 0 |
| `ready_undrawn_max` | 0 |
| `ready_awaiting_upload_max` / `ready_blocked_transition_max` | 0 / 0 |
| `committed_missing_max` | 0 |
| `frontier_pipeline_pending` during the walk | 0–20 (its `2197` maximum is the initial settle) |
| `stale_loads` / `stale_meshes` / `hard_cap_blocks` / `upload_failures` | 0 / 0 / 0 / 0 |
| `presentation_commit_failures` / `commit_invariant_failures` | 0 / 0 |
| `anchor_rejections` | 0 |
| slides / blocked moves over the whole walk | 0 / 0 |

**The one-job-per-poll rail keeps up with a walking player, with about a third of its capacity spare, and produced no coverage failure of any kind.** KI-017 is therefore not a blocker for traversal at the walk speed, and nothing was changed. If a run mode is ever proposed it roughly doubles the demand rate and this measurement has to be taken again.

### Renderer scale while walking

| measure | value |
|---|---|
| chunk meshes resident and drawn | 291–294 |
| GPU quads | 471,874–475,660 |
| presentation-owned chunk-mesh bytes | 84.9–88.6 MB, peak `93,268,896` |
| world draws | 291–294 chunks + 34 actor and weapon parts + 1 instanced effect draw |
| `renderer_render_wall`, mean / max | `10,649`–`10,967` µs / `13,902`–`18,018` µs |
| frame interval / observed rate | `16.666` ms / `60.00` FPS |

For comparison, M4's settled `depth-stack` pose recorded 204 meshes, 358,619 quads and 65,992,424 bytes. A body at eye level in the meadow draws about **44% more chunks and 32% more quads** than that pose, because a low camera sees more of the demand cube as non-empty. It is still vsync-bound at 60 FPS, so no bottleneck appeared and **no culling, batching or instancing work was done.**

### Combat cost while the domain runs all session

Unchanged from M6's headless figure: `5.463` µs per tick, `0.656` ms of one core per simulated second, about `10.9` µs per 60 Hz frame. Over the seventy-second walk the clock reported one capped frame and one dropped tick in total.

## M7E — evidence

### The played sessions

Three driven sessions on the audited host through the rebuilt harness, one client at a time, its own enumerated window, foreground asserted before every input and every capture, DPI-aware, `1920 x 991` client area.

**Session 1 — the whole loop.** Walked the route's corners from `(-68.5, 49.5)`, reached the adversary, fought, was defeated, was returned to the configured route start, **walked the entire route again**, and fought again. `defeats_player = 2`, `resets = 2`, and after each reset both bodies were at their configured starts. Zero ` ERROR ` lines, zero dropped combat events, frame events, particles or sound requests, zero audio device errors, exit code 0.

**The player-defeat path is therefore proven in the real client, twice.** The session continued past both.

**Session 2 — the fight, aimed at the body.** Same walk, then a fight loop that aimed at the adversary's reported position. `player_swings = 11`, `player_hits = 0`.

That is the harness, not the fight, and M6 already measured why: a driver that reads a position from a five-second report line and swings at where the body **was** aims no better than one that presses keys blind. M6's `combat-probe aim` settled the underlying question by standing a passive body at a bearing and swinging — a swing aimed at the body connects at every range the attack reaches. **The player-victory path is proven headlessly by `a_defeated_adversary_remains_and_the_session_continues`, and is not proven in the real client.** It is named here rather than implied, and it is one of the things the owner playtest will settle.

**Session 3 — the water.** Walked due south from the route start, straight at the river the `terrain-probe` locates at `z ~ 16`. The body stopped dead in `z` at `24.8` and travelled west along the shore. The waterline at that `x` is exactly there: `(-97, 24)` has no water voxel and `(-97, 23)` does. **The barrier lands on the drawn waterline, which is TRAVERSE-001 shown rather than asserted.**

### A finding the run produced: a body held against a barrier slides, it does not block

The first water run reported `blocked_moves_player = 0` for the whole time the body was pinned at the shore, because `try_move` refused the full move, refused the `z` component, accepted the `x` component, and returned `slid` rather than `blocked`. A log could not distinguish a free walk from a body held against water.

`CombatCounters::slid_moves` now counts it, and the re-run reported `slid = 195` on the tick the body reached the waterline and `728` by the end of the push. `CombatCounters` is not part of the encounter trace, so `GOLDEN_ENCOUNTER_SIGNATURE` did not move; the probe confirms all three M6 values byte-identical.

### A finding the captures produced: the follow camera looks into a descending bank

At the shore the captures came back with the lower half of the frame filled by a flat green plane, the far valley above it, and **no player in the frame at all**. The camera telemetry added to answer it says why: body at `(-74.8, 24.8)` standing on ground at about `15.9`, camera at `(-73.5, 19.5, 30.8)` standing over ground at about `19.0`. The camera is `3.58` above the body's ground, which is `0.5` above its own, and at `FOLLOW_PITCH = -0.24` a sightline from half a unit up meets the ground about two units out — well short of a body six units away.

This is terrain between the camera and the subject, which is KI-025, and KI-025 is explicitly out of M7's scope. A clamp against the ground under the body was written, measured to move the camera `0.05` world units in this case, and **reverted** rather than kept as a fix that does not fix anything. The finding is recorded as **KI-026** with the numbers that characterise it.

What was kept is the telemetry: `camera_x`, `camera_y`, `camera_z` and `camera_detached` are in the `combat state` line, because working backwards from a screenshot to a camera position is exactly what this cost.

### Visual assessment, at eye level

Captures opened and inspected, not inferred from counters.

- **Route start.** The body reads from behind at conversational distance: rust tunic, carried sword, a full pip row above the head. The forest pocket around it has depth — trunks at several distances, canopy shadows on the grass, a highland silhouette through the gap. A trunk stands directly between camera and head, which is KI-025 again and is the ordinary case of it.
- **Meadow and the long views.** The valley reads as a valley: a broad grass floor, scattered trees and shrub cubes, a rock band, and the far side rising. The horizon carries the style bible's atmospheric compression.
- **Contact.** Both bodies legible and told apart by size and by tunic hue, both pip rows readable, shadows grounded. At contact range the two bodies visually overlap into one mass from behind — the separation is holding them at their capsule radii and the camera is simply behind one of them. M6 records this as part of KI-025.
- **The shore.** The waterline is a hard edge with a rock band behind it. What a player is given at that edge is an invisible wall: there is no wading, no splash, no shoreline cue, and the body simply stops. That is the accepted consequence of the owner's water decision and it is now something a person can judge.

Nothing about terrain art was changed. The captures came first and the assessment is the artefact.

### What the owner still has to judge

No agent can close this. The questions, in the owner's words:

- Is walking through the world legible?
- Does the terrain work at eye level?
- Does the route feel like traversal rather than a debug corridor?
- Is there dead time? The named route is `63.9` seconds of walking at `3.40` u/s, and **running was deliberately not added** — the design's ninety-second figure was never an approved threshold and the decision is a judgement about feel, not a number.
- Is the blocked water comprehensible?
- Does the camera let you navigate? KI-026 says what it does on a descending bank.
- Does walking up to the enemy and entering combat read as one coherent session?
- Does continuing after the fight work?

## Regressions at this HEAD

| regression | result |
|---|---|
| M3 diagnostic (`WORLD=diagnostic`, `PROFILE=m3-diagnostic`) | exit 0, 0 errors, 0 validation/device-lost/panic, `gaps_closed=0`, 0 commit failures |
| M4 golden | exit 0, same, clean |
| M5 character (`CHARACTER=course`) | exit 0, clean; identity `0xb21b87d0a3ce9078`, geometry `0x3ebe8c822f549151`, 16 parts, 1,814 quads, 16 world and 16 shadow draws, 1,280 dynamic bytes — the M5 figures unchanged |
| M6 `off` | 0 `combat state` lines, 0 `encounter ready`, **0 audio devices opened** |
| M6 `script` | exit 0, reports, audio open |
| M6 `moment:confirmed-hit` and `+6` | both freeze at tick `499` |
| M6 `armed` | exit 0, reports, audio open |

**Locked signatures.** `combat-probe signature` reports all three M6 values byte-identical: weapon geometry `0x8a6b18edd4a2b879`, weapon identity `0x084bf386500bb0e4`, golden encounter `0x64157522d2535658`. Every M5 signature is unchanged, and the character suite proves it on every run. M6's `OWNER LISTENING: PASS` and `OWNER PLAYTEST: PASS` remain valid: the synth, the action curves, the telegraph, the camera response and the combat rules are untouched.

The workspace has **690 tests**, three of them `#[ignore]`d and run deliberately: the M6 listening fixture, the whole-region reachability report, and the adversary placement re-derivation.

## Accepted limitations

New in M7: **KI-026**, the follow camera on a descending bank. Everything M6 and M5 left open is unchanged, including KI-025 and KI-006/KI-021 — one host, one adapter, no automated image comparison.
