# Current handoff

Last updated: 2026-09-18

## Current position

**M1, M2, M3, M4, and M5 are all merged into `main`.** M4 — Beautiful Terrain Vertical Slice landed through [PR #8](https://github.com/Jovinull/veldwake/pull/8) at merge commit `abadadff6ad1251e4f291d272577da6121d37540`, after independent branch QA, a green pull-request CI run, and a green post-merge CI run on the merge commit ([run 35281261587](https://github.com/Jovinull/veldwake/actions/runs/35281261587)). M3D landed through PR #7 at `bfc9db1eec085390f9148efbb2a14d61d1fa0d6e`; M3C through PR #6 at `c669929b00427c2f438529b572931400a24b6d3d`.

The repository now renders one deterministic 800 x 96 x 800 voxel region — a verdant highland valley with a meandering river, a pond, banded cliffs, forest pockets, and low vegetation — streamed around a free-fly camera and lit by a directional sun with a filtered shadow map, a procedural sky, height-aware fog, and two weather states. It is still an engineering proof: there is no persistence, no character, no collision, and no gameplay.

**M5 — Procedural Character is complete and merged.** It landed through [PR #9](https://github.com/Jovinull/veldwake/pull/9) at merge commit `5bebc4fa5195b427216a296fff810a294b7bbd7b`, parents `abadadff6ad1251e4f291d272577da6121d37540` and `3c0406dfafc503aa1dc6d98ab1b92db8c33e0e07`, with a green post-merge CI run on the merge commit ([run 35352715367](https://github.com/Jovinull/veldwake/actions/runs/35352715367)). It added the `veldwake-character` crate, the `CHARACTER_STYLE.md` contract, [ADR-0004](../adr/0004-rigid-voxel-character-and-analytical-locomotion.md), the client adapter and render path, and the milestone document with its captures assessed in writing. Final clean D3D12 capture/motion/lifecycle QA accepted KI-018, KI-019, and KI-020 as documented; M3/M4 regressions and M5 lifecycle passed with no validation, device-loss, fatal, or panic marker. The remote branch `feat/m5-procedural-character` is preserved at `3c0406dfafc503aa1dc6d98ab1b92db8c33e0e07`.

## Start here — reading order for a session with no prior context

Read these before changing anything. They are the whole truth of the project; nothing important about it lives outside the repository.

1. [`../../AGENTS.md`](../../AGENTS.md) — the constitution, binding on every agent runtime
2. [`../../CLAUDE.md`](../../CLAUDE.md) — the Claude Code entry point and runtime-specific notes
3. [`../PROJECT_STATE.md`](../PROJECT_STATE.md) — what exists, what does not, which milestone is active
4. This file
5. [`../planning/ROADMAP.md`](../planning/ROADMAP.md) — milestone sequence and the accepted scope of each
6. [`../engineering/ARCHITECTURE.md`](../engineering/ARCHITECTURE.md) — crate map, boundaries, and the test a new crate must pass
7. [`../engineering/INVARIANTS.md`](../engineering/INVARIANTS.md) — the rules a change may not break
8. [`../engineering/DETERMINISM.md`](../engineering/DETERMINISM.md) — named streams, fingerprints, what determinism does and does not promise
9. [`../engineering/PERFORMANCE.md`](../engineering/PERFORMANCE.md) — measurement method and every recorded number, with its host
10. [`../engineering/TESTING_STRATEGY.md`](../engineering/TESTING_STRATEGY.md) — what is tested and why, and the current test count
11. [`../engineering/OBSERVABILITY.md`](../engineering/OBSERVABILITY.md) — what the client and the probes report
12. [`../procedural/PROCEDURAL_PHILOSOPHY.md`](../procedural/PROCEDURAL_PHILOSOPHY.md) — how generated content is expected to be built and justified
13. [`../procedural/WORLD_GENERATION.md`](../procedural/WORLD_GENERATION.md) — the proposed world pipeline and how little of it M4 implemented
14. [`../audiovisual/ART_DIRECTION.md`](../audiovisual/ART_DIRECTION.md) — the aesthetic direction
15. [`../audiovisual/STYLE_BIBLE.md`](../audiovisual/STYLE_BIBLE.md) — the versioned, checkable constraints M4 was held to, and what they explicitly do not cover
16. [`../audiovisual/CHARACTER_STYLE.md`](../audiovisual/CHARACTER_STYLE.md) — the versioned, checkable constraints M5 was held to, and how they relate to the style bible
17. [`../planning/M5_PROCEDURAL_CHARACTER.md`](../planning/M5_PROCEDURAL_CHARACTER.md) — the milestone that just closed, its evidence, its measurements and its limitations
18. [`../adr/0004-rigid-voxel-character-and-analytical-locomotion.md`](../adr/0004-rigid-voxel-character-and-analytical-locomotion.md) — why body parts are rigid and locomotion is analytical, and what that costs
19. [`../KNOWN_ISSUES.md`](../KNOWN_ISSUES.md) — every accepted limitation, including the four M5 ones
20. [`../LEARNINGS.md`](../LEARNINGS.md) — reusable discoveries below ADR scope
21. [`EVIDENCE_HARNESS.md`](EVIDENCE_HARNESS.md) — how a capture is produced and what makes one invalid
22. [`WORKFLOW.md`](WORKFLOW.md) — how an agent is expected to work in this repository

Then, as needed: [`../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md`](../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md), the rest of the [ADRs](../adr/README.md), and [`../environment/SETUP.md`](../environment/SETUP.md) for gate commands and environment variables.

Then the code, in this order, because it is the surface M6 has to work against:

- `crates/character` — the descriptor, compiler, skeleton, locomotion, ground contact, IK and collision representation;
- `apps/client/src/character.rs` — the entire boundary between the character and the world: the `GroundSampler` adapter over `TerrainField`, the diagnostic courses, the stand points and the camera poses;
- `apps/client/src/renderer.rs` and the WGSL in `apps/client/src/` — the world and shadow passes and the one shared lighting function;
- `apps/client/src/camera.rs` and `apps/client/src/input.rs` — a free-fly camera and keyboard state, with no player-character relationship of any kind;
- `crates/procedural` and `apps/client/src/world.rs` — how terrain reaches the client.

## Continue here — M6 — Combat Slice

**M6 is the next milestone and nothing of it exists.** No combat code, no weapon, no enemy, no AI, no hit model, no controller, and no dependency for any of them has been added.

The accepted scope, copied from [`../planning/ROADMAP.md`](../planning/ROADMAP.md) and deliberately not expanded:

> One weapon and one enemy with movement, attack/defense or dodge, telegraphs, hit reaction, camera response, procedural impact audio, VFX, animation, and encounter/readability playtest evidence.

That is a statement of intent, not a plan. Start by reading the repository and assessing the current state, exactly as M5 was started; `ARCHITECTURE.md` carries the test a new crate must pass before it exists, and `AGENTS.md` carries the rule for new dependencies.

### What is deliberately not decided

None of the following has been chosen, and nothing in the repository implies a choice. Do not treat silence as a decision, and do not adopt one by writing code that assumes it:

weapon representation; enemy architecture; whether there is an AI state machine, a behaviour tree, or neither; navigation; the hitbox and hurtbox model; the damage model; stamina; dodge invulnerability frames; the combat controller; physics integration of any kind, including whether Rapier is ever introduced; animation architecture beyond what M5 already does; a VFX system; audio event architecture; camera shake implementation; target lock; whether an ECS is introduced at all; and the entity model.

These are for a session that investigates them against the consolidated repository. Earlier conversations contain speculation about several of them and none of that speculation is a decision.

### What M5 leaves you to build on

Facts, not suggestions:

- `veldwake-character` compiles one humanoid from a descriptor and poses it. It is headless, has no GPU or platform types, and does **not** depend on `veldwake-procedural`. A second body — an enemy — is a second descriptor before it is anything else.
- The one coupling between a character and the world is the two-method `GroundSampler` trait, implemented by the client in eleven lines over `TerrainField`. Anything M6 needs from the world should be examined the same way before a crate reaches for another crate.
- Locomotion is analytical and driven by distance, not by clips. There is no animation graph and no blend tree. An attack animation is not a clip to drop in; decide deliberately whether it becomes a second analytical layer or the thing that finally justifies an animation system, and record that decision as an ADR rather than in code.
- Collision is a **representation** — one capsule and one box per part — and nothing consumes it. There is no collision response, no sweep, no query, and no broadphase. M6 is adding all of that from nothing if it needs it.
- The camera is free-fly and the input module is keyboard state. Neither knows a character exists. There is no character controller and no player relationship at all.
- The character is drawn through the same shared WGSL lighting function as terrain, with static geometry uploaded once and one small uniform per part per frame. Anything new that draws should be examined against that pattern before inventing a second lighting path.
- `VELDWAKE_CHARACTER` and `VELDWAKE_POSE` drive the diagnostic scenes; `SETUP.md` lists every value. Those courses and camera poses are fixtures and are the cheapest way to get a reproducible frame of a character.

### What M5 deliberately did not build

Faces beyond two eye voxels, hair or clothing as geometry, equipment, weapons, a second archetype, character LOD, a character controller, player input, gameplay or camera integration, any physics or collision library, ragdoll, cloth, an animation graph, emotion, hand IK, and a save format. Those are later milestones and the merged branch must not be read as having prejudged any of them.

## Questions before context reset

None. Everything a session starting M6 needs is in the repository: the merged state and its SHAs, the accepted scope of M6 and the explicit list of decisions left open, the crate boundaries and the test a new crate must pass, the invariants, the determinism rules, the measurement method with its host, the visual contracts, the evidence harness, and every accepted limitation as a numbered open issue rather than as prose.

Nothing material about the project's real state exists only in a conversation. The M5 implementation decisions that outlive the milestone are in [ADR-0004](../adr/0004-rigid-voxel-character-and-analytical-locomotion.md); the ones that do not are in the milestone document with the capture or the measurement that produced them.

## Immediate risks

- **M5's visual validation is one host and one adapter, and it is merged anyway.** The independent D3D12 exit gate passed on the audited Intel Iris Xe machine and nothing compares captures automatically (KI-021). Visual readability is not reducible to the headless fixtures, so a later reviewer disagreeing with a judgement in the milestone document is a legitimate finding, not a re-litigation.
- **Do not start M6 by deciding its architecture from a previous conversation.** The list of open decisions above is deliberate. Investigate each against the merged repository.
- **The character's visual contract is versioned and now locked.** `CHARACTER_STYLE_VERSION`, `CHARACTER_COMPILER_VERSION` and `CHARACTER_SCHEMA_VERSION` fold into a character's identity fingerprint, and seven fixture signatures are checked against the compiler on every test run. Moving a voxel, a bone or a gait constant without bumping the matching version is a failing test, which is the intent. Re-lock deliberately; never re-lock to make a test pass.
- **Character identifiers are `128..192`.** Terrain is `64..128` and the M2/M3 diagnostics are `1`, `2`, `7`. The client is the only place all three tables are visible and it carries the disjointness test. A new content domain declares its own range there.
- **`veldwake-character` must not gain a dependency on `veldwake-procedural`.** It duplicates thirty lines of hashing rather than reach for that crate's helpers, deliberately and with the reason written at the duplication. The one thing it needs from the world is `GroundSampler`, which the client implements in eleven lines over `TerrainField`.
- **`GroundSampler` returns the top face of the topmost solid voxel, and that is a contract, not an implementation detail.** Smoothing it makes soles float or sink against the blocks a viewer can actually see. Water is not ground.
- **Gait thresholds are in leg lengths per second.** Absolute world-unit thresholds silently put the same body at a different size into the wrong gait; that is why they were changed. `GaitParameters::speed_for` converts back.
- **The diagnostic courses are closed loops and are sampled modulo their own duration.** A course that runs once has always finished before a seventy-five-second settle fires a capture. If a leg's duration or speed changes, the loop must still return to its own start position and facing, and a test says so.
- Characters are outside the style bible. `audiovisual/STYLE_BIBLE.md` says so explicitly in *What this document does not cover*: characters, creatures, equipment, animation, and particles are later work and must not be invented there. M5 either extends that document deliberately, with the same kind of checkable rules, or writes its own and says how the two relate. Do not silently reuse terrain rules for a character and call it consistent.
- Terrain material identifiers start at `64` (`procedural::material::FIRST_TERRAIN_ID`) precisely so the M2/M3 diagnostic identifiers `1`, `2`, and `7` stay distinguishable. Any new content domain needs its own declared range and its own round-trip test; do not extend the terrain enum by accident.
- There is no collision, no physics, no ground contact, and no character controller anywhere in the repository. The client camera is a free-fly camera. A milestone that needs terrain contact is adding all of that from nothing, and the voxel field it would query is `veldwake-procedural`'s height field, not a physics world.
- The M4 visual captures were not committed. `agents/EVIDENCE_HARNESS.md` records the procedure, the traps, and the validity rules so they can be reproduced; `procedural::region::GOLDEN_POSES` records where they were taken from. A written assessment in the milestone document is the durable artefact, not the PNGs.
- Do not turn the documented future crate map into empty crates. M3D deliberately added no crate: the cache has one consumer, needs no build isolation, and inverts no dependency, so it lives in `crates/streaming`. Re-argue that from the crate test in `ARCHITECTURE.md` before splitting it out.
- The disk cache is discardable by definition. Never let a cache failure reach the runtime as data loss, turn I/O or corruption into AIR, or treat a missing file as `KnownAbsent`; absence is a typed entry. A rejected entry falls back to the source even when deletion or publication fails. Preserve the separate `rejected_entries_removed` and `rejected_entry_delete_failures` evidence.
- Changing `DiagnosticChunkSource::load` must make the exhaustive finite-corpus behavioral-signature test fail. Update `SOURCE_BEHAVIOR_SIGNATURE` and the locked runtime fingerprint deliberately; bump `SOURCE_SCHEMA_REVISION` when the semantic contract changes beyond output bytes. A descriptor-only locked value is not sufficient.
- A cache key has one deterministic logical value, but temp-and-rename is not physically write-once on every platform: Windows normally refuses replacement while Unix may atomically replace an existing target. Concurrent publishers must remain semantically equivalent. Do not promise a cross-platform winner; preserve atomic visibility and validation instead.
- `ChunkCache::open` sweeps temporaries and walks the footprint synchronously. In the client it runs once during opt-in event-loop initialization, not in the frame hot path. Per-chunk reads, decodes, fallback, and publication run on the worker. Do not broaden the claim to “the cache never touches the event-loop thread.”
- Cache counters live in `RuntimeMetrics::cache` and must stay separate from the streaming counters, and all zero when no cache is configured. That contract is what keeps M3B and M3C evidence comparable.
- Do not let the M2 voxel representation or mesher depend on `wgpu`, `winit`, or the diagnostic camera.
- The M1 cube has been replaced by the M2 fixture; its palette and framing remain diagnostic presentation, not game art direction.
- Keep chunk dimensions, material encoding, coordinate order, and mesh winding explicit and tested; accidental conventions will become expensive compatibility constraints.
- The M2 convenience mesher explicitly uses `BoundaryPolicy::Expose`; future streaming work must use deliberate availability policy rather than silently equating “not loaded” with AIR.
- Negative world coordinates use Euclidean division and are locked at `0`, `31`, `32`, `-1`, `-32`, `-33`, plus range extrema. Preserve this contract.
- One model uniform/bind group and a linear fixture lookup per chunk are intentionally limited M3A diagnostics, not accepted scalable batching or residency designs.
- M3B1 does not equate an unavailable/loading neighbor with known AIR. Jobs carry a global non-reused request token plus center/neighbor generations; results are accepted only while every stamp remains current.
- The default 27/81/125 demand counts, 160-payload cap, one worker, and observed probe timing are diagnostic evidence, not target-world performance promises. Measure before expanding workers, residency, or upload throughput.
- Keep drawability and deallocation separate in the bridge: a presented mesh whose data is no longer current (content change, unload, neighbor unloaded or reloaded, unavailable neighbor, leaving render demand) is deactivated in the same update, before uploads and regardless of the release budget. Only a presentation-only change (`LodChange`, `NeighborPresentation`, `Membership` while still in render demand) whose target `differs_only_by_presentation` may keep the committed mesh drawn, and then only until its transition group commits. Never reorder that to "upload first" and never widen the retained set.
- A group commits on both sides or on neither. `commit_group_with` validates unique/non-empty ready membership while holding the runtime's mutable borrow across the presentation's all-or-nothing `commit_staged_group` callback and CPU commit. The bridge map changes only after success. Never split this into independently fallible mutations or reintroduce per-coordinate commit.
- Chunk-mesh GPU claims must use the highest simultaneous `committed + staged` observation at mutation boundaries, especially after stage and before commit. This metric is presentation-owned chunk meshes only, not global GPU memory; depth, pipelines, debug resources, and driver/wgpu retention are excluded.
- Debug views are off by default. `Off` guarantees 0 per-frame debug primitive allocations, 0 debug uniform writes, 0 debug draws, and untouched mesh upload budget—not absence of the fixed debug pipeline/unit buffer or reusable slots retained after use. One bind group and one draw per box costs 15–38 FPS at 637 boxes (KI-012).
- The debug module reads state, it never derives it. Record states come from `tracked_states()`, presented levels from the bridge's committed stamps, groups from `transition_groups()`. A renderer-side guess about streaming state would be a new source of truth.
- Transition groups join two adjacent dirty chunks only across a drawn side whose level or seam contract toward the other changes (`seam_changes`). Keep the split coverage counters in every LOD benchmark. `frontier_pipeline_pending` is not-yet-known geometry; `ready_awaiting_upload` and `ready_blocked_transition` are known non-empty geometry not visible yet; `committed_missing` is a hard invariant failure and must remain zero. Any non-zero `ready_undrawn` is a temporary coverage hole and must be reported, even when bounded and seam-safe (KI-014).
- The seam-coherence checker's rule is part of the contract: a drawn contract toward an undrawn neighbor in render demand must match that neighbor's target level unless the drawn chunk's own replacement toward that face is pending. Do not "fix" a checker failure by weakening it to "any undrawn neighbor is fine".
- A run's zero-gap counter is not visual proof. Mid-movement captures (and burst captures assembled into contact sheets when a hole is suspected) are the evidence; a counter that only sees chunks that were drawn before cannot see entrants that never appeared.
- `ChunkPresentation` has exactly two implementors (renderer, test double). It is a testability seam, not a render abstraction; do not add methods the bridge does not call.
- Camera anchoring must stay `floor`-based with explicit non-finite/out-of-range rejection; `as i64` truncates toward zero and would misplace every negative sub-voxel position.
- With one worker, `finalize_evictions` before `dispatch_one` means an eviction backlog cannot trigger `hard_cap_blocks`; the cap holds through accounting. Re-validate that reasoning if worker count or poll order ever changes.
- M3C must never let a coarse level hide a known crack or turn an unavailable neighbor into AIR; the seam rule is decided in the plan and tested per direction and occupancy case before any rendering work.
- Keep every new mesher or slab path generic over `EDGE`; the 32-edge fingerprints and topology tests are the tripwire for accidental edge-specific code.
- `Lod1` meshes are in 16-cell units; the renderer applies `scale.x = 2` in the model uniform and never scales the translation. Any new presentation path must keep both levels spanning the same `32³` world volume (test `both_levels_span_the_same_world_volume_at_positive_and_negative_chunks`).
- Benchmarks and driven smokes need an idle desktop: injected keys reach only the foreground window. The bench script re-asserts focus before every input and aborts as invalid when it cannot; a run with `cpu_evictions = 0` on the traversal path did not move and must be discarded.
- The seam rule's orientation convention is `from_neighbor`'s: `face` is the direction from the center, and slab constructors pick the neighbor's touching layers themselves. The first seam oracle got this backwards; the tests now encode the convention.
- Retries are for the host linker lock only (`LNK1104`, KI-008). A failing assertion, panic, test, Clippy, or build error is evidence and is never re-run until it passes.
- The no-LOD wider-radius baseline is part of M3C's evidence, not an afterthought; LOD stays enabled only if the recorded decision rule passes on the audited host.
- Treat integrated Intel Iris Xe as one conservative host, not the target matrix.
- Do not change repository visibility, replace/configure remotes, publish releases, choose a license, or present the working title as cleared without owner authorization.
