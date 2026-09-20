# M7 — Traversable Region

Status: **in progress on `feat/m7-traversable-region`**
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
| M7A | movement/domain correctness: exact support height, `MoveBlockReason`, `TraversalLegality`, optional arena, explicit initial positions, player-victory policy, `WorldContact` | pending |
| M7B | canonical voxel-water predicate, `TerrainWalkability`, TRAVERSE-001, reachability audit, named route, route signature, runtime route proof, enemy placement | pending |
| M7C | playable traversal client: `VELDWAKE_ENCOUNTER=traverse`, body-anchored streaming, dormancy reporting, outcome policies | pending |
| M7D | measurement and visual/motion QA on the audited host | pending |
| M7E | documentation and exit evidence | pending |

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
