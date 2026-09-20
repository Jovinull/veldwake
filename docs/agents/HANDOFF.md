# Current handoff

Last updated: 2026-09-20

## Current position

**M7 — Traversable Region is implemented on `feat/m7-traversable-region` and is waiting on the owner.** The branch is based on `docs/post-m6-handoff` at `aac3ec55519d93e44397af549eb18089c7dcdfcd`, deliberately, so the two post-M6 documentation commits travel into the same pull request. **No pull request has been opened.** The one thing outstanding is a fresh **OWNER PLAYTEST**; M6's does not transfer and no agent can close it.

Everything else is done: 690 tests pass, every gate is green, the M3/M4/M5/M6 regressions are clean, and every M5 and M6 locked signature is byte-identical — `combat-probe signature` reports all three M6 values unchanged.

Read [`../planning/M7_TRAVERSABLE_REGION.md`](../planning/M7_TRAVERSABLE_REGION.md) before touching anything M7 built. Its two durable corrections are **MOVE-001** (movement authority reads exact support surfaces, never the smoothed pelvis) and **TRAVERSE-001** (water is the voxel predicate, not the continuous field relation); both are in [`../engineering/INVARIANTS.md`](../engineering/INVARIANTS.md).

**M1 through M6 are all merged into `main`.** M6 — Combat Slice landed through [PR #10](https://github.com/Jovinull/veldwake/pull/10) at merge commit `f840ff7880e1857e86b3a74c4d3f66ceaf82a922`, whose parents are `5bebc4fa5195b427216a296fff810a294b7bbd7b` and `296621dcdff9d93e92c3ccca0b21d2c822e418aa`, after independent branch QA, a green pull-request CI run ([run 35416586910](https://github.com/Jovinull/veldwake/actions/runs/35416586910)), and a green post-merge CI run on the merge commit ([run 35417282890](https://github.com/Jovinull/veldwake/actions/runs/35417282890)). The remote branch `feat/m6-combat-slice` is preserved at `296621dcdff9d93e92c3ccca0b21d2c822e418aa`.

**M1, M2, M3, M4, and M5 are all merged into `main`.** M4 — Beautiful Terrain Vertical Slice landed through [PR #8](https://github.com/Jovinull/veldwake/pull/8) at merge commit `abadadff6ad1251e4f291d272577da6121d37540`, after independent branch QA, a green pull-request CI run, and a green post-merge CI run on the merge commit ([run 35281261587](https://github.com/Jovinull/veldwake/actions/runs/35281261587)). M3D landed through PR #7 at `bfc9db1eec085390f9148efbb2a14d61d1fa0d6e`; M3C through PR #6 at `c669929b00427c2f438529b572931400a24b6d3d`.

The repository renders one deterministic 800 x 96 x 800 voxel region — a verdant highland valley with a meandering river, a pond, banded cliffs, forest pockets, and low vegetation — streamed around the camera and lit by a directional sun with a filtered shadow map, a procedural sky, height-aware fog, and two weather states. Since M5 a generated humanoid stands and walks in it, and since M6 a person can fight in it: a player-controlled combatant and one adversary, a procedurally generated weapon, kinematic movement against the ground rules, hit and hurt queries that decide a swing, damage, stagger, knockback, a third-person follow camera, voxel-chip effects, a diegetic health readout and synthesised impact audio. The free-fly camera is still there and is still the default; the encounter is opt-in behind `VELDWAKE_ENCOUNTER`.

It remains an engineering proof and a vertical slice rather than a game. What does not exist: persistence of any kind, progression, a world beyond this one region, a second enemy, weapon or archetype, inventory, quests or economy, networking, a mod runtime, menus or a UI framework, and any generalized collision or physics simulation — combat collision is two specific queries, not a physics world.

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
17. [`../audiovisual/COMBAT_STYLE.md`](../audiovisual/COMBAT_STYLE.md) — the versioned, checkable constraints M6 was held to: the weapon, the five action poses, the two effects, the readout, and what the camera may do
18. [`../design/COMBAT.md`](../design/COMBAT.md) — the design intent combat aims at, most of which M6 deliberately does not build yet
19. [`../planning/M7_TRAVERSABLE_REGION.md`](../planning/M7_TRAVERSABLE_REGION.md) — the milestone on the branch: the owner decisions it answers to, the two architectural corrections it carries, the whole-region reachability result, the named route and everything measured
20. [`../planning/M6_COMBAT_SLICE.md`](../planning/M6_COMBAT_SLICE.md) — the milestone before it: its evidence, its measurements, the six branch-QA findings, the owner gates, and its limitations
21. [`../planning/M5_PROCEDURAL_CHARACTER.md`](../planning/M5_PROCEDURAL_CHARACTER.md) — the milestone before it, and the character every combat pose is built on
22. [`../adr/0005-fixed-step-headless-combat-domain.md`](../adr/0005-fixed-step-headless-combat-domain.md) — why combat is integer-stepped and headless, and why two combatants are the whole entity model
23. [`../adr/0006-action-pose-layer-beside-analytical-locomotion.md`](../adr/0006-action-pose-layer-beside-analytical-locomotion.md) — why a tick-driven action layer sits beside distance-driven locomotion, and why ADR-0004 was not superseded
24. [`../adr/0007-procedural-impact-audio-boundary.md`](../adr/0007-procedural-impact-audio-boundary.md) — why the synth has no I/O, and where the device boundary is
25. [`../adr/0004-rigid-voxel-character-and-analytical-locomotion.md`](../adr/0004-rigid-voxel-character-and-analytical-locomotion.md) — why body parts are rigid and locomotion is analytical, and what that costs
26. [`../KNOWN_ISSUES.md`](../KNOWN_ISSUES.md) — every accepted limitation and every closed one, with the reason each was closed
27. [`../LEARNINGS.md`](../LEARNINGS.md) — reusable discoveries below ADR scope
28. [`EVIDENCE_HARNESS.md`](EVIDENCE_HARNESS.md) — how a capture is produced, what makes one invalid, and how it picks the right window
29. [`WORKFLOW.md`](WORKFLOW.md) — how an agent is expected to work in this repository

Then, as needed: [`../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md`](../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md), the rest of the [ADRs](../adr/README.md), and [`../environment/SETUP.md`](../environment/SETUP.md) for gate commands and environment variables.

Then the code. Read it in this order, because it is the surface anything after M6 works against:

- `crates/combat` — the fixed-step headless domain. One call to `Encounter::step` is one tick; every duration is a tick count; the weapon, the two attack specs, the swept hit query, the torso-column hurt volume, the kinematic movement rules, the adversary's state machine and the bounded event type all live here. It has no GPU, window, audio or filesystem types, and it depends on `crates/character` and never the reverse.
- `crates/character` — the descriptor, compiler, skeleton, distance-driven locomotion, ground contact, IK and collision representation, plus the tick-driven action pose layer M6 added beside it. `pose()` is M5's path and is unchanged; `pose_with()` takes an action overlay.
- `apps/client/src/encounter.rs` — where the domain meets the frame: the integer tick accumulator, the encounter modes including the frozen named moments, and the bounded per-frame event buffer.
- `apps/client/src/arena.rs` — where in the world a fight happens, and how a named camera pose is placed against the two bodies rather than against the map.
- `apps/client/src/input.rs` and `apps/client/src/camera.rs` — **both camera models live here.** With the encounter off the free-fly camera and raw keyboard state are exactly what they were before M6. With an encounter armed, input is latched on the key-down edge and interpreted relative to a third-person follow camera, and `HitShake` moves that camera on a confirmed hit and on nothing else.
- `apps/client/src/vfx.rs`, `readout.rs`, `synth.rs`, `audio.rs` — presentation that is **driven by `CombatEvent` and never by a guess**: chips and a telegraph accent from one fixed pool, the eight-pip health readout, the pure synth with no I/O, and the one cpal device behind a lock-free queue.
- `apps/client/src/renderer.rs` and the WGSL beside it — the world, shadow, character and effect passes, and the one shared lighting function terrain and bodies both call. Actors and weapons upload once; a frame writes transforms.
- `crates/procedural`, `crates/streaming` and `apps/client/src/world.rs` — how terrain is generated, made resident, and reaches the client. Combat touches this only through the `GroundSampler` trait.

The shapes worth holding while reading: the domain is authoritative and headless, the client advances it in whole ticks, and every visible or audible consequence of a fight is derived from an event the rules published.

## Continue here — M7 needs a person, then a pull request

**Do not open a pull request and do not start another milestone.** M7's exit gate is the owner playing it, and the order the owner set is: review the implementation, play it, then a PR.

To play it:

```text
VELDWAKE_ENCOUNTER=traverse cargo run --release -p veldwake-client
```

The world takes about a minute to settle. The body starts at the route start `(-69, 49)`; the adversary stands `162` steps away at `(14, 191)`, dormant until you come within `14` world units of it. `WASD` walks relative to the camera, the mouse looks, `J` or left mouse attacks, `K` or `Space` dodges once the adversary is awake, `F4` detaches the camera. The named route is `217.09` world units and `63.9` seconds of walking.

The questions that belong to a person and to nobody else:

- Is walking through the world legible?
- Does the terrain work at eye level? Every M4 and M5 visual claim was made from a flying camera at a scanned pose; this is the first time the region is judged from inside it.
- Does the route read as traversal, or as a debug corridor?
- Is there dead time? **Running was deliberately not added.** The design's ninety-second figure was never an approved threshold, the measured walk is `63.9` seconds, and whether that is a walk or a wait is a judgement about feel.
- Is blocked water comprehensible, or an invisible wall? There is no wading, no splash and no cue — KI-028.
- Does the camera let you navigate? KI-026 says what it does on ground that descends behind you.
- Does walking up to the enemy and entering combat read as one coherent session?
- Does continuing after the fight work?

### What is proven and what is not

| claim | evidence |
|---|---|
| the route is walkable | `the_route_is_walkable_in_a_real_encounter` drives a real encounter tick by tick over the real adapters; and three driven client sessions walked it |
| water blocks, on the drawn waterline | a driven session walked into the river and stopped at `z = 24.8`, where `(-97, 24)` has no water voxel and `(-97, 23)` does |
| the adversary sleeps, then wakes | 10,000 ticks at 200 units leave it bit-identical with no stream draw; a driven session woke it by walking up to it |
| **player defeat** returns both bodies to their configured starts and the session continues | proven in the real client **twice**, in one session that then walked the whole route again |
| **player victory** leaves the adversary down and the session going | proven **headlessly only**. Three driven sessions landed zero hits out of twenty-six swings, which is what a driver aiming from a five-second report line measures, not a property of the fight — M6 settled that question with `combat-probe aim`. **This is the one path the owner playtest should exercise deliberately.** |

### What M7 leaves you to build on

Facts, not suggestions:

- **A smoothed presentation value must never decide an authoritative rule.** `base_height` is the filtered pelvis height and movement was comparing steps against it; at `tau = 0.12 s` a body climbing at gradient `g` carries about `0.42 g` of lag, and with `max_step_up` exactly one voxel *any* lag makes a legal step illegal. MOVE-001 exists so this cannot come back.
- **`movement::check_move` is the only implementation of movement legality**, it returns `MoveBlockReason`, and the reachability audit calls it. Do not write a second copy to answer "why did that fail".
- **`GroundSampler` is unchanged and must stay unchanged.** It answers where the visible solid surface is, including the river bed inside a river. Whether a body may walk there is `TraversalLegality`, a veto consulted about a move's destination only.
- **The audit is a topological upper bound.** It evaluates every step from a settled stand. The only proof that a body driven by intent gets somewhere is simulating it.
- **The region splits in two and the session starts in the smaller half** (KI-027): `307,144` standable columns reachable against a larger component of `314,861`. The highland *is* reachable, 22 steps from the start, and the prediction that it would not be was wrong. The barrier attribution — water `2,464`, step-up `44,495`, max-drop `1,273` — is where any milestone that wants the whole region navigable should start.
- **A body held against a barrier slides; it does not block.** `slid_moves_*` exists because a log watching only `blocked_moves` reported zero while a body was pinned at a waterline for twenty-three units of shore.
- **Streaming keeps up with a walking player with a third of the rail spare** — 25–42 loads a second against about 60 — with zero gaps, zero `ready_undrawn` and zero commit failures. KI-017 was measured, not fixed. A run mode roughly doubles that and needs the measurement again.
- **No culling was added.** A body at eye level draws 291–294 chunk meshes and 471,874–475,660 quads against M4's settled 204 and 358,619, and it is still vsync-bound at 60 FPS. There was no bottleneck to fix.

## Superseded — the pre-M7 continuation point

**M6 is merged and there is no next milestone.** That is deliberate, and it is the first thing to understand before doing anything else.

[`../planning/ROADMAP.md`](../planning/ROADMAP.md) does **not** define an M7. What it defines is a set of *later capability groups*:

> Persistence/world editing, aggregate/local world simulation, settlements/history/economy, richer procedural assets/audio/music, multiplayer transport, and WASM modding follow only after the central technical and fun risks are proven. Split and order them when earlier evidence exists; do not manufacture detailed milestones now.

So the next session's job is **not** to pick one and start. It is to read what now exists, weigh it against those groups, and *propose* the next milestone — scope, exit criteria, and what it deliberately will not build — for the owner to accept before any code is written. Do not create a `feat/m7-*` branch, do not name a milestone M7, and do not treat the order of the list above as a decision. It is a list, not a queue.

### What now exists

The repository is a playable vertical slice of one fight in one deterministic region:

- A finite 800 x 96 x 800 voxel region generated from a seed, streamed around the camera, lit with a sun, a shadow map, a procedural sky, fog and two weather states.
- A humanoid compiled from a descriptor: sixteen rigid voxel parts on a sixteen-bone skeleton, analytical gaits driven by distance travelled, soles solved against the ground with two-bone IK.
- A fight: two combatants, one procedurally generated sword, an adversary that telegraphs and commits, a swing decided by sweeping the blade against a torso-column volume, damage, stagger, knockback, hitstop, a camera that moves only on a confirmed hit, voxel chips, a diegetic health readout, and synthesised impact audio through one cpal device.
- Evidence machinery: named camera poses, named frozen combat moments with tick offsets, five headless probes, locked fingerprints and behavioural signatures across five crates.

### What still does not exist

No persistence of any kind — no saves, no world edits, no encounter state that survives a run. No second enemy, weapon, archetype or biome beyond what M5 and M6 build. No inventory, stats, progression, quests or economy. No menu, HUD framework, pause or death screen. No networking, no multiplayer, no mod runtime, no editor. No physics engine and no collision simulation — movement is kinematic against a ground query. No aggregate world simulation, settlements or history. No music, reverb or mixer. No automated visual regression, and one host and one adapter for every visual claim in the repository (KI-006, KI-021).

### What M6 leaves you to build on

Facts, not suggestions:

- **A fixed-step authoritative domain exists and works.** `Encounter::step` takes no duration; the client's accumulator is integer; twenty seconds at 30, 60 and 144 Hz produce the same 2,400 ticks and the same trace. Anything else that needs deterministic simulation should copy this shape rather than invent a second one.
- **Presentation reads events and never infers.** Damage, stagger, knockback, hitstop, the reaction pose, chips, sound and the camera impulse all come from one `CombatEvent`. That boundary is [ADR-0002](../adr/0002-presentation-independent-authority.md) applied to a fight and it is the reason the audio cross-check comes out exact.
- **Two animation categories coexist**: distance-driven locomotion and a tick-driven action layer, recorded in [ADR-0006](../adr/0006-action-pose-layer-beside-analytical-locomotion.md), which also records why ADR-0004 was *not* superseded.
- **Audio has a boundary, not a system.** A pure synth with no I/O, a lock-free ring of atomics, and one thin device adapter ([ADR-0007](../adr/0007-procedural-impact-audio-boundary.md)). There is no mixer and no asset path, and adding either is a decision rather than an extension.
- **Identifier ranges are declared and proven disjoint**: diagnostics `1`, `2`, `7`; terrain `64..128`; character `128..192`; weapon `192..224`. A new content domain declares its own range and adds its own test to the client, which is the one place all four are visible.
- **The evidence harness has a written method** in [`EVIDENCE_HARNESS.md`](EVIDENCE_HARNESS.md), including how a capture picks the right window. Branch QA caught the previous harness photographing a browser; the traps are recorded there rather than rediscovered.

### The owner gates, both closed

**OWNER LISTENING: PASS** and **OWNER PLAYTEST: PASS**, 2026-09-19. The owner played the offline audio fixture — the hit reads as an impact, the whiff is distinguishable from it, no clipping, click or problematic distortion — and played the armed encounter in the real client, finding the telegraph, attack, dodge, impact and camera legible enough for M6, and the slice *simple but functional, and the direction intended*.

Those judgements belong to a specific build. A change to the synth, the action curves, the camera or the telegraph invalidates them and needs a fresh one; no test will notice. The fixture is how it gets remade:

```text
cargo nextest run -p veldwake-client --run-ignored all the_listening_fixture
```

### What was decided in M6, and where the reasoning lives

Every item the pre-M6 handoff listed as open has an answer in code and a written reason. Do not re-open one without reading the reason first:

| question | answer | where |
|---|---|---|
| weapon representation | a descriptor of physical identity only; timing lives in `AttackSpec` | `crates/combat/src/weapon.rs`, and the milestone's *two corrections* |
| enemy architecture | a four-state machine producing intent, never a second timeline | `crates/combat/src/adversary.rs` |
| AI: state machine, behaviour tree, or neither | a state machine, with named deterministic decision streams | ADR-0005 |
| navigation | none. The adversary steers, the arena is a disc, there is no navmesh | milestone non-goals |
| hitbox and hurtbox model | a swept blade segment against one torso-column capsule refitted per tick | `crates/combat/src/hurt.rs`, KI-022 |
| damage model | integer health, one damage figure per spec, no resistances | `crates/combat/src/spec.rs` |
| stamina | none | milestone non-goals |
| dodge invulnerability frames | none. A dodge succeeds by geometry or not at all | ADR-0005, and KI-023 for why that reads as evasion — closed, premise refuted |
| combat controller | latched edge input, camera-relative movement, facing follows movement | `apps/client/src/input.rs` |
| physics integration | none, and no Rapier. Kinematic movement against `GroundSampler` | ADR-0005 |
| animation architecture | a five-action keyed pose layer beside M5's locomotion, no graph | ADR-0006 |
| VFX system | none. Two effects, one fixed pool, one instanced draw | `apps/client/src/vfx.rs` |
| audio event architecture | one `CombatEvent`, a lock-free queue, a pure synth, one device | ADR-0007 |
| camera shake | tick-driven, capped, confirmed hits only | `apps/client/src/camera.rs` |
| target lock | none | milestone non-goals |
| ECS | no. Two combatants indexed by `Side`, and that is the entity model | ADR-0005 |

## Questions before context reset

None. Everything a session with no prior context needs is in the repository: the merged state and its SHAs, what M6 built and what it deliberately did not, the crate boundaries and the test a new crate must pass, the invariants, the determinism rules, the measurement method with its host, the visual contracts, the evidence harness with its traps, and every accepted limitation as a numbered open issue rather than as prose.

Nothing material about the project's real state exists only in a conversation. The decisions that outlive M6 are in [ADR-0005](../adr/0005-fixed-step-headless-combat-domain.md), [ADR-0006](../adr/0006-action-pose-layer-beside-analytical-locomotion.md) and [ADR-0007](../adr/0007-procedural-impact-audio-boundary.md); the ones that do not are in [`../planning/M6_COMBAT_SLICE.md`](../planning/M6_COMBAT_SLICE.md) beside the capture or the measurement that produced them. The branch-QA findings, the closed-loop play, the owner gates and both re-locked signatures are all recorded there with their reasons.

## Immediate risks

- **M7's exit gate is a person, and the failure mode is opening a pull request without it.** Review, play, then PR — that is the order the owner set.
- **The player-victory path has never been seen in the real client.** It is proven headlessly and it is the first thing a playtest should try, because a driver that reads a position from a report line aims at where the body was.
- **Do not re-lock `GOLDEN_ROUTE_SIGNATURE` to make a test pass.** It covers the world identity, the compiled movement spec, `TRAVERSAL_RULE_VERSION`, both endpoints, every waypoint and every checkpoint, so a changed `max_step_up` moves it even when the path does not. Every re-lock carries OLD/NEW/WHY beside the constant, exactly as M5 and M6 require.
- **`ADVERSARY_COLUMN` is a cache of a derivation, not a hand-picked pair.** `the_derived_placement_is_the_locked_one` is `#[ignore]`d because the search costs about fourteen seconds; run it deliberately after anything that touches the terrain, the movement spec or the placement rules.
- **The whole-region audit runs in the ordinary test set and the placement search does not.** That split came from measurement — `590` ms to sample plus `273` ms to audit, against about `14` seconds to search — and no test asserts a duration. Do not add one.
- **A camera on ground that descends behind the body looks into the bank** (KI-026). A clamp was written, measured at `0.05` world units of correction, and reverted rather than kept as a fix that fixes nothing. Any real fix raises the camera until its sightline clears the near ground, which is an occlusion problem and belongs to a camera milestone.
- **Whether an exact partition hits the catch-up cap depends on how the spare nanoseconds fall**, not only on the frame rate. `combat-probe partition` front-loads them and measures zero capped frames at thirty hertz; spreading them does not. Both deliver exactly twenty seconds. Do not quote one distribution as a property of the rate.
- **`CombatCounters` is not part of the encounter trace.** That is why `slid_moves` could be added without moving `GOLDEN_ENCOUNTER_SIGNATURE`. Anything that goes into `run_script`'s per-tick bytes *does* move it.
- **The traversal dodge gate must stay inside the tick loop.** A frame runs up to four ticks and the first can cross the aggro radius; hoisting the brain-state read would make a dodge depend on how the frames were cut, which is the one thing the integer clock exists to prevent.

- **There is no next milestone, and inventing one is the failure mode.** `ROADMAP.md` lists later capability groups and says explicitly not to manufacture detailed milestones before the evidence exists. Propose, get acceptance, then build.
- **The owner's judgement is recorded, not re-derivable.** OWNER LISTENING: PASS and OWNER PLAYTEST: PASS are a person's assessment of a specific build, kept beside the measurements rather than folded into them. A change to the synth, the action curves, the camera or the telegraph invalidates that judgement and needs a fresh one; no test will notice.
- **A swing now aims itself, inside bounds, and that is a gameplay decision rather than a convenience.** `AIM_ASSIST_CONE` is `35°` and `AIM_ASSIST_RANGE` is the reach of the attack. It exists because closed-loop play measured one hit in four from positions the aim table says connect: facing follows movement, so a body that stands still to swing cannot track one that is moving, while the adversary's brain steers continuously. Widening it turns the fight into a lock; removing it makes the fight unwinnable by aiming. Change it only against a new measurement.
- **A named moment must check what its name claims.** `successful-dodge` asserted only that a dodge was in progress while a blade was live, and the captures taken at it were of a dodge that failed. Any new moment predicate gets the same scrutiny: the name is a claim about the frame.
- **`GOLDEN_ENCOUNTER_SIGNATURE` and `GOLDEN_ACTION_POSE_SIGNATURE` each moved twice, and every move has a written OLD/NEW/WHY beside the constant.** All five M5 signatures are byte-identical and `CHARACTER_STYLE_VERSION` was deliberately not bumped, because it is part of a character's identity and the action rules answer to `COMBAT_STYLE_VERSION` instead. Re-lock deliberately; never to make a test pass.
- **Weapon identifiers are `192..224`.** Terrain is `64..128`, characters `128..192`, and the M2/M3 diagnostics are `1`, `2`, `7`. The client carries the global disjointness test and a new content domain declares its own range there.
- **The hurt volume is not the M5 body capsule and must not be confused with it.** `BodyCapsule` keeps two bodies out of each other; `hurt::HurtVolume` decides damage. Sweeping the blade against the first is the defect that produced hits in empty air, and the module documentation carries the three rounds of measurement that chose the second.
- **`Encounter::step` takes no `dt` and must not learn to.** One call is one tick. Every duration in the domain is a tick count and authored seconds are compiled by a validating constructor, which is what makes `NaN`, negative and infinite time unrepresentable rather than rejected.
- **The audio callback's contract is checkable, not aspirational.** No allocation, no lock, no logging, no blocking, no panic. `std::sync::mpsc` is not documented to be allocation-free and must not replace the atomic ring; the ring needs no `unsafe`, which the workspace forbids anyway.
- **The cpal dependency enables no backend features.** WASAPI is compiled into the Windows backend with no flag. Adding `asio`, `jack`, `pipewire`, `pulseaudio` or `realtime` is a dependency decision with its own audit, not a convenience.
- **A named moment's camera frames the fight, not the arena.** `arena::frame_the_fight` reinterprets a pose's offset in the fight's own frame, because a fight is wherever it drifted to. A capture that reverts to a fixed world offset will sooner or later photograph one body standing in front of the other.
- **M5's visual validation is one host and one adapter, and it is merged anyway.** The independent D3D12 exit gate passed on the audited Intel Iris Xe machine and nothing compares captures automatically (KI-021). Visual readability is not reducible to the headless fixtures, so a later reviewer disagreeing with a judgement in the milestone document is a legitimate finding, not a re-litigation.
- **Do not decide the next milestone's architecture from a previous conversation.** M6's decisions are settled and recorded above; the milestone after it has none yet, and speculation in an old conversation is not one. Investigate against the merged repository and propose.
- **The character's visual contract is versioned and now locked.** `CHARACTER_STYLE_VERSION`, `CHARACTER_COMPILER_VERSION` and `CHARACTER_SCHEMA_VERSION` fold into a character's identity fingerprint, and seven fixture signatures are checked against the compiler on every test run. Moving a voxel, a bone or a gait constant without bumping the matching version is a failing test, which is the intent. Re-lock deliberately; never re-lock to make a test pass.
- **Character identifiers are `128..192`.** Terrain is `64..128` and the M2/M3 diagnostics are `1`, `2`, `7`. The client is the only place all three tables are visible and it carries the disjointness test. A new content domain declares its own range there.
- **`veldwake-character` must not gain a dependency on `veldwake-procedural`.** It duplicates thirty lines of hashing rather than reach for that crate's helpers, deliberately and with the reason written at the duplication. The one thing it needs from the world is `GroundSampler`, which the client implements in eleven lines over `TerrainField`.
- **`GroundSampler` returns the top face of the topmost solid voxel, and that is a contract, not an implementation detail.** Smoothing it makes soles float or sink against the blocks a viewer can actually see. Water is not ground.
- **Gait thresholds are in leg lengths per second.** Absolute world-unit thresholds silently put the same body at a different size into the wrong gait; that is why they were changed. `GaitParameters::speed_for` converts back.
- **The diagnostic courses are closed loops and are sampled modulo their own duration.** A course that runs once has always finished before a seventy-five-second settle fires a capture. If a leg's duration or speed changes, the loop must still return to its own start position and facing, and a test says so.
- Characters are outside the style bible. `audiovisual/STYLE_BIBLE.md` says so explicitly in *What this document does not cover*: characters, creatures, equipment, animation, and particles are later work and must not be invented there. M5 either extends that document deliberately, with the same kind of checkable rules, or writes its own and says how the two relate. Do not silently reuse terrain rules for a character and call it consistent.
- Terrain material identifiers start at `64` (`procedural::material::FIRST_TERRAIN_ID`) precisely so the M2/M3 diagnostic identifiers `1`, `2`, and `7` stay distinguishable. Any new content domain needs its own declared range and its own round-trip test; do not extend the terrain enum by accident.
- **There is still no physics engine and no collision simulation, and that is not the same as no collision.** What exists is specific and narrow: ground contact through the `GroundSampler` trait since M5, kinematic movement rules and two collision queries since M6 — a swept blade against a hurt volume, and a separation that keeps two bodies out of each other. All of it is closed-form against `veldwake-procedural`'s height field. A milestone that needs general collision response, projectiles, stacking or anything resembling rigid-body dynamics is adding that from nothing, and should re-read [ADR-0005](../adr/0005-fixed-step-headless-combat-domain.md) before assuming a physics engine is the answer.
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
