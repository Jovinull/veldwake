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
