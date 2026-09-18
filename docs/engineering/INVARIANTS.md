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
