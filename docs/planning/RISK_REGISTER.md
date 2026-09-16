# Risk register

Qualitative ratings avoid false precision. Review at milestone boundaries.

| ID | Risk | Likelihood | Impact | Mitigation | Warning signs |
|---|---|---|---|---|---|
| R-001 | Scope overwhelms delivery | High | Critical | Capability milestones, explicit non-goals, one proof slice before breadth | Many half-built subsystems; milestones expand while in progress |
| R-002 | Overengineering/custom-engine work displaces the game | High | High | Point-of-need abstractions/dependencies; vertical evidence | Job system/ECS/framework built before concrete consumers |
| R-003 | Procedural visuals are incoherent or unattractive | High | Critical | Style bible, constrained grammars, previews, visual fixtures, human review | Parameter count grows without silhouette/palette rules |
| R-004 | Procedural audio/music sounds artificial or fatiguing | High | High | Early listening prototypes, motifs/harmony, DSP budgets, bounded exceptions | Random-note logic, clipping, constant density, listener fatigue |
| R-005 | Combat is deferred behind engine work | Medium | Critical | M6 combat slice; prototype timing/feedback early in character work | Creature generators grow before one good encounter exists |
| R-006 | World simulation costs too much or produces invisible detail | High | High | Aggregate/local levels; consequence-value test; budgets and inspection | Full NPC updates far away; state with no player-facing effect |
| R-007 | Multiplayer authority is retrofitted too late | Medium | Critical | Authority boundary from M0; local-server-compatible commands/state | Client presentation mutates canonical world directly |
| R-008 | Too many crates/dependencies fragment ownership | High | Medium | Logical domains first; physical boundary requires evidence | Empty crates, cyclic convenience adapters, slow broad rebuilds |
| R-009 | Premature optimization damages clarity | Medium | High | Representative benchmarks/profiles; preserve reference paths | Unsafe/SIMD/custom allocator without measured bottleneck |
| R-010 | Save/generator evolution strands worlds | Medium | Critical | Explicit versions, migrations, canonical data/cache split, goldens | Persisted structs serialized directly with no version |
| R-011 | Visual changes are hard for AI agents to assess | High | High | Stable scenes, screenshots, perceptual diff, style constraints | “Looks good” claims without comparable captures |
| R-012 | Agents introduce conflicting architecture and rapid debt | High | High | AGENTS constitution, ADRs, boundaries, DoD, scoped reviews | Duplicate systems, silent requirement changes, lint/test weakening |
| R-013 | Feature creep dilutes Wonder/Mastery/Consequence | High | High | Pillar test and roadmap tradeoff for every major feature | Backlog grows without removing or delaying anything |
| R-014 | No traditional asset pipeline becomes dogma that harms quality | Medium | High | Procedural-first, not absolute purity; explicit narrow exceptions | Poor output retained only to preserve “zero assets” claim |
| R-015 | Travel through a large world becomes dead time | Medium | High | Progressive traversal, route decisions, measure friction | Players avoid expeditions or rely entirely on fast travel |
| R-016 | Integrated dev host hides vendor/backend issues | Medium | High | Adapter diagnostics, later hardware matrix and CI/manual coverage | Renderer assumptions tied to Intel/D3D12 behavior |
| R-017 | Working-title collision or licensing ambiguity blocks release | Medium | High | Formal name clearance and owner license decision before publicity | Branding/public contributions begin under assumptions |
