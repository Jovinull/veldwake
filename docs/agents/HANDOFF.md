# Current handoff

Last updated: 2026-09-18

## Current position

**M1, M2, M3, and M4 are all merged into `main`.** M4 — Beautiful Terrain Vertical Slice landed through [PR #8](https://github.com/Jovinull/veldwake/pull/8) at merge commit `abadadff6ad1251e4f291d272577da6121d37540`, after independent branch QA, a green pull-request CI run, and a green post-merge CI run on the merge commit ([run 35281261587](https://github.com/Jovinull/veldwake/actions/runs/35281261587)). M3D landed through PR #7 at `bfc9db1eec085390f9148efbb2a14d61d1fa0d6e`; M3C through PR #6 at `c669929b00427c2f438529b572931400a24b6d3d`.

The repository now renders one deterministic 800 x 96 x 800 voxel region — a verdant highland valley with a meandering river, a pond, banded cliffs, forest pockets, and low vegetation — streamed around a free-fly camera and lit by a directional sun with a filtered shadow map, a procedural sky, height-aware fog, and two weather states. It is still an engineering proof: there is no persistence, no character, no collision, and no gameplay.

**M5 — Procedural Character is implemented on `feat/m5-procedural-character` and has not been reviewed by anyone but the agent that wrote it.** The branch adds the `veldwake-character` crate, the `CHARACTER_STYLE.md` contract, [ADR-0004](../adr/0004-rigid-voxel-character-and-analytical-locomotion.md), the client adapter and render path, and the milestone document with its captures assessed in writing. Nothing is merged and no pull request is open.

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
16. [`../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md`](../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md) — the milestone that just closed, its evidence and its limitations

Then, as needed: [`../KNOWN_ISSUES.md`](../KNOWN_ISSUES.md), [`../LEARNINGS.md`](../LEARNINGS.md), [`EVIDENCE_HARNESS.md`](EVIDENCE_HARNESS.md), [`WORKFLOW.md`](WORKFLOW.md), the [ADRs](../adr/README.md), and [`../environment/SETUP.md`](../environment/SETUP.md) for gate commands and environment variables.

## Continue here — independent QA of the M5 branch

One continuation point: **review `feat/m5-procedural-character` as an independent reviewer, then decide whether it merges.** The branch is complete against the accepted scope and its own exit criteria; what it has never had is a second pair of eyes.

Read [`../planning/M5_PROCEDURAL_CHARACTER.md`](../planning/M5_PROCEDURAL_CHARACTER.md) first, then [`../audiovisual/CHARACTER_STYLE.md`](../audiovisual/CHARACTER_STYLE.md) and [ADR-0004](../adr/0004-rigid-voxel-character-and-analytical-locomotion.md). The milestone document is written so that the five design errors captures rejected — the voxel scale, the chest width, the missing neck, the sleeve that was a voxel too narrow, and a character that walked backwards — are recorded with the frame that rejected each one. Re-deriving those is wasted work; finding the sixth is the job.

Where to push hardest, in order:

1. **Run the client and look at it.** `VELDWAKE_CHARACTER` takes `off`, `idle`, `course`, `slope`, `slope-stand`, `sturdy`, `pose:<name>`, `walk:<0..7>` and `run:<0..7>`; `VELDWAKE_POSE` takes any of the ten `character-*` camera poses. Every visual claim in the milestone document was made by opening a capture and magnifying it, and every one of them is a judgement a reviewer may disagree with.
2. **The contact claim on moving ground.** Standing contact is clean and asserted. KI-018 records what a planted foot does when it crosses a terrace during stance, and the bound in that test is the number to attack.
3. **The joint-overlap invariant and what it costs.** KI-020 is its visible consequence at a flexed knee. A reviewer who thinks the sliver is worse than recorded should say so with a magnified frame.
4. **Scope.** The branch adds one crate, no third-party dependency, no ECS, no physics engine, and no animation graph. Check that nothing crept in.

### What was deliberately not built

Faces beyond two eye voxels, hair or clothing as geometry, equipment, a second archetype, character LOD, a character controller, player input, gameplay integration, any physics or collision library, ragdoll, cloth, an animation graph, and a save format. Those are later milestones and the branch must not be read as having prejudged them.

## Questions before context reset

None. Everything a reviewer needs is in the repository: the accepted scope, the contract the captures were judged against, the evidence, the measurements with their host, and the limitations as open issues rather than as prose.

## Immediate risks

- **The M5 branch has had no independent review.** Every judgement in its milestone document — that the scale reads, that the silhouette reads, that the walk cycle is a walk — was made by the agent that also wrote the code. The headless tests are objective; the visual assessment is not, and it is the exit criterion.
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
