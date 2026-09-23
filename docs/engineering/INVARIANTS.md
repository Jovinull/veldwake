# Engineering invariants

Status: **Accepted**. Changes require evidence and, when structural, a superseding ADR.

**ARCH-001 — Renderer independence**  
Authoritative simulation code must not depend on renderer code.

**ARCH-002 — Headless generation**  
World generation must never require a GPU, window, or presentation service.

**ARCH-003 — Authority boundary**  
Server and authoritative gameplay must not depend on client presentation; renderer state cannot define authoritative game state.

**WORLD-001 — Addressable generation**  
Given the same compatible world version, generator version, canonical inputs, and seed, world generation must produce equivalent specified results.

**WORLD-002 — Position-addressed generation**  
Every generation decision must be a function of world position, the seed, and the declared configuration. No generator may consume a sequential random source, observe iteration order, or depend on which chunk asked. Implemented and tested in `veldwake-procedural` since M4.

**WORLD-003 — Hydrology by construction**  
Where water exists without a solver, its surface must be a continuous monotone function of position, so that it cannot run uphill and cannot step at a chunk boundary. A water rule whose correctness depends on tuning rather than on construction is not acceptable.

**CONTENT-001 — Declared identifier ranges**
Every content domain must own a declared, contiguous `VoxelId` range and a total round trip between its semantic material and that range. Terrain holds `64..128` and characters `128..192`; the M2/M3 diagnostic identifiers `1`, `2` and `7` remain outside both. A domain may not extend another domain's enum, and the disjointness is asserted where all the tables are visible at once — currently the client. Implemented and tested since M5.

**CHAR-001 — A body is one piece**
Every non-root part of a compiled character must contain its own joint origin and overlap its parent, so that no rotation of any joint can open a hole in the body. This is a property of the compiled volumes and is asserted against them, not a property of any pose. Implemented and tested in `veldwake-character` since M5.

**CHAR-002 — Ground is what the viewer can see**
A character's ground query returns the top face of the topmost solid voxel of a column, not a smoothed height field, and water is never ground. A contact model that disagrees with the blocks the renderer draws is wrong however elegant it is. Implemented and tested since M5.

**TRAVERSE-001 — Water is what the viewer can see**
A column is untraversable by water exactly when the generator writes at least one water voxel in it, which is `TerrainSample::has_water_voxel()`: `water_surface_y() > surface_y()`. The continuous relation `water_surface > height` — `is_submerged()` — is a different question and disagrees on a band of quantized columns where a viewer sees dry ground. A traversal predicate that follows the continuous relation blocks ground the player is standing on, and is wrong however elegant it is. This is CHAR-002 applied to traversal. Implemented and tested since M7.

**MOVE-001 — Movement authority reads exact support surfaces**
A step's legality is decided between the exact support surface under the body and the exact support surface at the destination, both from `GroundSampler::surface`. It is never decided against `CharacterState::base_height`, which is the **smoothed** height the pelvis follows and is presentation. Judging a step against the filter made the same step legal standing still and illegal while walking, because the lag is a function of how long the body had been climbing. One function, `movement::check_move`, is the only implementation of the rule, and it returns why a move was refused so that an audit can attribute a barrier without owning a second copy of the conditions. Implemented and tested in `veldwake-combat` since M7.

**LAND-001 — A landmark is written, never carved**
A landmark writes its voxels only into air. It never replaces a terrain voxel, never excavates the ground and never moves a surface: a column that carries a landmark rests on the terrain that was already there, and the foundation fills the gap between the ground and the base course rather than sinking into it. The generator asserts this where it writes, and two tests prove it from the generated voxels: `a_landmark_rests_on_terrain_in_every_column_it_fills` reads under and over every filled column, and `qa_a_landmark_only_ever_replaces_air` differences the whole neighbourhood against a world of the same identity composed without landmarks — `524,288` voxels compared, `1,740` written over air or over a plant the reservation had already removed, and no terrain or water voxel moved. Implemented and tested since M8.

**LAND-002 — A visible wall refuses a body, and an opening does not**
Solid landmark geometry is untraversable: if a viewer can see stone there, `TraversalLegality` refuses a destination whose body would occupy it, with the same `MoveBlockReason::Traversal` water produces. The converse binds equally — where the drawn geometry leaves room for the body, traversal must admit it, which is why a gate's opening is proved walkable by a search over the real surface grid rather than assumed. The keep-out is the widest body's capsule, measured from the compiled rigs in the client; no character dimension appears in `veldwake-procedural`, and `GroundSampler` is unchanged, so a landmark is never a surface to stand on. Held up by oracles rather than by agreement: `qa_the_keep_out_agrees_with_the_voxels_a_viewer_can_see` checks the veto against the drawn solids in both directions, and `qa_a_real_encounter_walks_a_body_through_the_gate_and_into_its_pillars` drives the authoritative loop through the opening and into a pillar. Implemented and tested since M8.

**PERF-001 — No blocking frame I/O**  
Blocking disk or network I/O must not occur on the render/game-frame hot path.

**SAVE-001 — Explicit versions**  
Every persisted format must carry an explicit version and define its compatibility/migration behavior before release use.

**AGENT-001 — Durable discovery**  
Materially relevant discoveries must be documented before a task is complete.

**DOC-001 — Documentation consistency**  
A change that makes current-behavior documentation false must update it in the same work.

**QUAL-001 — Honest gates**  
Agents must report checks as PASS, FAIL, BLOCKED, or NOT YET APPLICABLE based on actual execution; tests and assertions cannot be weakened merely to obtain green.

**COMBAT-001 — Integer time**  
An authoritative combat step takes no duration. `Encounter::step` advances exactly one tick, and every duration inside the domain — attack phases, dodge, stagger, hitstop, adversary timers, the defeat hold — is a tick count. Authored seconds exist only in configuration and are compiled to ticks by a validating constructor before they can reach the runtime, so a `NaN`, negative or infinite duration is unrepresentable rather than rejected, and a phase boundary is an integer comparison. Implemented and tested in `veldwake-combat` since M6.

**COMBAT-002 — Presentation may not invent a consequence**  
Damage, stagger, knockback, hitstop, the reaction pose, impact effects, impact audio and the camera impulse all originate from one `CombatEvent` produced by the rules. Presentation reads events; it never infers that a hit probably happened. A miss produces no damage, no hit reaction, no impact effect and no camera movement, and may produce a whiff sound. Implemented and tested in `apps/client` since M6.

**COMBAT-003 — Bounded work per tick and per frame**  
Every buffer in the combat path is fixed at compile time and every overflow is counted rather than absorbed: events per tick, events per frame, blade sweep substeps, simulation ticks per rendered frame, particles in the pool, sound requests in the queue, and simultaneous voices. Nothing in the path allocates per tick or per frame. Implemented and tested in `veldwake-combat` and `apps/client` since M6.

**COMBAT-004 — The real-time audio callback**  
The audio callback never allocates, never takes a lock, never logs, never blocks and never panics. Communication with it is a lock-free single-producer single-consumer ring of atomics, and everything it reports is an atomic the game thread reads on its own time. A channel that is not documented to be allocation-free does not satisfy this. Implemented in `apps/client/src/audio.rs` since M6.

**COMBAT-005 — Adversary initiative never answers the player's action**  
The adversary's decisions read the player's **position** and whether the player is defeated, and nothing else about the player: never its `Action`, attack phase or elapsed ticks, its input or latches, a future hit or position, the camera, or a weapon's identity as a shortcut for its reach. An adversary dodge is started only by the adversary's own state — the end of its own stagger, or its own lunge having connected. That trigger does not look at the player, so what keeps the dodge out of a player's swing is timing, not a check: at the authored numbers the player is still locked in a recovery or a stagger when the dodge starts. `CombatCounters::dodges_during_unresolved_swing` counts every adversary dodge that starts while the player has a swing that has begun, is not past its active window and has not connected, and it must stay `0`; a retune that makes it move has made the dodge look reactive even though it is not. Unit tests prove the first half structurally, by holding the adversary's state fixed and varying only the player's action; the counter is `0` in every oracle fight and every real-client session recorded for the branch. Implemented on `feat/combat-initiative-spacing` in `crates/combat/src/adversary.rs`, not merged. `feat/combat-pressure` drafted a different COMBAT-005 that let the adversary read the player's committed action after a reaction delay; that branch is blocked, the draft never reached `main`, and this one is its opposite on purpose.
