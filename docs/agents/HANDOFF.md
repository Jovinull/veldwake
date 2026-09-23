# Current handoff

Last updated: 2026-09-23

## Current position

**Combat Initiative / Spacing is implemented on `feat/combat-initiative-spacing` and passed the owner's playtest on 2026-09-23: OWNER PLAYTEST — PRESSURE DEMANDS RESPONSE: PASS.** Technical gates and real-client self-QA are green; the next step is an independent QA of the whole branch. No pull request is open, nothing is merged, and it has no milestone number — the owner approved it as an unnumbered slice. Base: `main` at `ee35f62f97afbe3d001a27a576e9bae21e77c4d2`. Read [`../planning/COMBAT_INITIATIVE.md`](../planning/COMBAT_INITIATIVE.md) before touching it: the three mechanisms, every number and the measurement that moved it, the oracles, the terrain results, the real-client evidence table, and what this agent could not verify.

**The one thing to hold:** the capability exists only when a tuning authors `adversary_pressure`. The historical encounter, every fixture, script and lock is byte-identical — `GOLDEN_ENCOUNTER_SIGNATURE` is still `0x6415_7522_d253_5658` — and the new behaviour is locked separately at `COMBAT_INITIATIVE_SIGNATURE = 0x8238_2662_d859_8cf3`. The two are never compared.

**M8 — Discoverable Landmarks is merged.** It landed through [PR #12](https://github.com/Jovinull/veldwake/pull/12) at merge commit `ee35f62f97afbe3d001a27a576e9bae21e77c4d2`. `docs/post-m8-handoff` carries a post-merge documentation commit that is not in `main`; this branch restates the merge in its own documents and will overlap it textually if both land.

**Two branches are frozen and must not be continued or merged from here.** `feat/m9-meaningful-reward`: M9's owner playtest failed on 2026-09-22, because approaching and attacking won with either weapon. `feat/combat-pressure`: blocked, because an adversary sidestep started after the player's swing is visible is fair only at `18`–`20` ticks of reaction. Combat initiative is the answer to both findings that acts **before** the player commits; see *Where it came from* in its document.

The M8 record below is kept because its owner evidence and its known weak points still bind.

**M8 — the discovery playtest and branch QA.**

**OWNER DISCOVERY PLAYTEST WITHOUT NAVIGATION INSTRUCTIONS: PASS.** The wording is exact and the exactness matters: during the session the owner was given no coordinate, direction, waypoint or route, but had read the implementation report beforehand, so the session was *uninstructed* rather than literally blind. Told only to play and explore normally, the owner noticed two structures with no navigation interface of any kind, read them as two different destinations — one a tower, one a broken construction — chose the tower deliberately, walked to it, **found the adversary while exploring**, and liked it. The structures were judged simple but good and sufficient for the current objective, with the observation that they still read somewhat close to the visual language of Cube World and familiar voxel RPGs.

**That is the whole point of the milestone, so it is worth stating against M7.** M7: walking the world works, and the owner found nothing in it. M8: the owner immediately noticed two different destinations, chose one, explored toward it, and found the adversary.

**Be exact about what is owner evidence and what is not.** The owner gave no distance measurement, no contrast judgement, no judgement of the third landmark, no clear-versus-overcast comparison, no judgement of the gate's passability and no judgement of KI-032. All of those are technical and self-QA evidence, and promoting any of them to owner judgement is the same mistake M7's handoff exists to prevent.

**Branch QA has been done and found one defect**, now fixed: `CompiledMonolith::new` accepted a descriptor it could not compile — `band_period = 0` divided by zero and negative dimensions compiled into an empty landmark — because it trusted a convention the type system did not enforce. It now validates and returns `DescriptorError`, and `PlanError::Descriptor` carries it. QA also added eight oracles that judge the landmarks from the drawn voxels rather than from the plan, re-derived every number the milestone claims, walked a body through the gate in the authoritative loop, and corrected three numbers in the documentation.

Read [`../planning/M8_DISCOVERABLE_LANDMARKS.md`](../planning/M8_DISCOVERABLE_LANDMARKS.md) first if you are continuing that work: it carries the composition, the numbers, the locks it moved, the owner's session, the branch QA section, and what nobody could verify.

**M7 — Traversable Region is complete and merged. M1 through M7 are all in `main`.** It landed through [PR #11](https://github.com/Jovinull/veldwake/pull/11) at merge commit `0c81c069bb95a8caf3b6a5252b89b023c6334ae1`, whose parents are `f840ff7880e1857e86b3a74c4d3f66ceaf82a922` and `2a747d859fe03db4a84e6f57d32035dd8e1feb4a`, after the owner's playtest, independent branch QA, a green pull-request CI run ([run 35630288272](https://github.com/Jovinull/veldwake/actions/runs/35630288272)) and a green post-merge CI run on the merge commit ([run 35632133251](https://github.com/Jovinull/veldwake/actions/runs/35632133251)). The remote branch `feat/m7-traversable-region` is preserved at `2a747d859fe03db4a84e6f57d32035dd8e1feb4a`, which is the head where the 699 tests and every gate were run. The branch was based on `docs/post-m6-handoff` at `aac3ec55519d93e44397af549eb18089c7dcdfcd` deliberately, so the two post-M6 documentation commits travelled into the same pull request.

**There is no next milestone.** That is deliberate and it is the first thing to understand before doing anything else.

**Two gates closed M7, and they are not the same gate. Do not conflate them.**

**OWNER PLAYTEST — TRAVERSAL EXPERIENCE: PASS**, 2026-09-21, by the repository owner: walking the world works, the terrain is sufficiently legible at eye level, the experience is enjoyable within the current scope, the walking duration does not justify adding running in this milestone, and locomotion feels somewhat stiff and raw but is acceptable for this slice. Water and the camera did not prevent enjoying the traversal. Small imperfections were noticed and none was judged a blocker.

**The owner did not find the adversary.** So the traversal gate is closed and nothing else is. **Exploration to combat, a player victory, and the session continuing past a victory are independent branch-QA evidence from the real client — never owner judgement**, and no document in this repository may say otherwise. KI-029 records why one entity in an 800 x 800 region with no discovery affordance can simply be missed.

**What branch QA proved in the real client**, in a closed observe-decide-input-observe loop with no `ScriptRunner` in it: the player walked the route, woke the adversary and defeated it (`adversary_health = 0`, `outcome = "adversary"`, `outcome_settled = true`); the encounter did **not** reset afterwards (`resets` `0` before the defeat hold and `0` after it), the body stayed at bit-identical coordinates, and the player walked `24.8` units away and kept playing; and in a second session the player was defeated, after which the reset put it **`0.000`** from its configured route start and the adversary **`0.000`** from the route goal, dormant again, and the session continued.

Read [`../planning/M7_TRAVERSABLE_REGION.md`](../planning/M7_TRAVERSABLE_REGION.md) before touching anything M7 built. Its two durable corrections are **MOVE-001** (movement authority reads exact support surfaces, never the smoothed pelvis) and **TRAVERSE-001** (water is the voxel predicate, not the continuous field relation); both are in [`../engineering/INVARIANTS.md`](../engineering/INVARIANTS.md).

**M1 through M6 are all merged into `main`.** M6 — Combat Slice landed through [PR #10](https://github.com/Jovinull/veldwake/pull/10) at merge commit `f840ff7880e1857e86b3a74c4d3f66ceaf82a922`, whose parents are `5bebc4fa5195b427216a296fff810a294b7bbd7b` and `296621dcdff9d93e92c3ccca0b21d2c822e418aa`, after independent branch QA, a green pull-request CI run ([run 35416586910](https://github.com/Jovinull/veldwake/actions/runs/35416586910)), and a green post-merge CI run on the merge commit ([run 35417282890](https://github.com/Jovinull/veldwake/actions/runs/35417282890)). The remote branch `feat/m6-combat-slice` is preserved at `296621dcdff9d93e92c3ccca0b21d2c822e418aa`.

**M1, M2, M3, M4, and M5 are all merged into `main`.** M4 — Beautiful Terrain Vertical Slice landed through [PR #8](https://github.com/Jovinull/veldwake/pull/8) at merge commit `abadadff6ad1251e4f291d272577da6121d37540`, after independent branch QA, a green pull-request CI run, and a green post-merge CI run on the merge commit ([run 35281261587](https://github.com/Jovinull/veldwake/actions/runs/35281261587)). M3D landed through PR #7 at `bfc9db1eec085390f9148efbb2a14d61d1fa0d6e`; M3C through PR #6 at `c669929b00427c2f438529b572931400a24b6d3d`.

The repository renders one deterministic 800 x 96 x 800 voxel region — a verdant highland valley with a meandering river, a pond, banded cliffs, forest pockets, and low vegetation — streamed around the camera and lit by a directional sun with a filtered shadow map, a procedural sky, height-aware fog, and two weather states. Since M5 a generated humanoid stands and walks in it; since M6 a person can fight in it — a player-controlled combatant and one adversary, a procedurally generated weapon, kinematic movement against the ground rules, hit and hurt queries that decide a swing, damage, stagger, knockback, a third-person follow camera, voxel-chip effects, a diegetic health readout and synthesised impact audio; and since M7 a person can **walk the whole of it**, with streaming anchored on the body, water blocking movement, an adversary placed by a reachability audit and asleep until you come near, and the session continuing whichever way the fight goes. The free-fly camera is still there and is still the default; the encounter is opt-in behind `VELDWAKE_ENCOUNTER`, whose traversal mode is `traverse`.

It remains an engineering proof and a vertical slice rather than a game. What does not exist: persistence of any kind, progression, a world beyond this one region, a second enemy, weapon or archetype, inventory, quests or economy, networking, a mod runtime, menus or a UI framework, any generalized collision or physics simulation — combat collision is two specific queries, not a physics world — any traversal verb beyond walking, and any affordance that points a player at anything.

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
17. [`../audiovisual/COMBAT_STYLE.md`](../audiovisual/COMBAT_STYLE.md) — the versioned, checkable constraints M6 was held to: the weapon, the six action poses (the lunge is the sixth), the two effects, the readout, and what the camera may do
18. [`../design/COMBAT.md`](../design/COMBAT.md) — the design intent combat aims at, most of which M6 deliberately does not build yet
19. [`../planning/COMBAT_INITIATIVE.md`](../planning/COMBAT_INITIATIVE.md) — the active, unnumbered slice on its branch, and [ADR-0010](../adr/0010-sixth-action-keeps-the-keyed-layer.md), why its lunge is still a keyed curve
20. [`../planning/M8_DISCOVERABLE_LANDMARKS.md`](../planning/M8_DISCOVERABLE_LANDMARKS.md) — the last merged milestone
21. [`../planning/M7_TRAVERSABLE_REGION.md`](../planning/M7_TRAVERSABLE_REGION.md) — the milestone before it: the owner decisions it answers to, the two architectural corrections it carries, the whole-region reachability result, the named route and everything measured
22. [`../planning/M6_COMBAT_SLICE.md`](../planning/M6_COMBAT_SLICE.md) — the milestone before it: its evidence, its measurements, the six branch-QA findings, the owner gates, and its limitations
23. [`../planning/M5_PROCEDURAL_CHARACTER.md`](../planning/M5_PROCEDURAL_CHARACTER.md) — the milestone before it, and the character every combat pose is built on
24. [`../adr/0008-traversal-legality-separate-from-ground-contact.md`](../adr/0008-traversal-legality-separate-from-ground-contact.md) — why a traversal veto sits beside `GroundSampler` instead of changing it, and why the veto never reports a height
25. [`../adr/0005-fixed-step-headless-combat-domain.md`](../adr/0005-fixed-step-headless-combat-domain.md) — why combat is integer-stepped and headless, and why two combatants are the whole entity model
26. [`../adr/0006-action-pose-layer-beside-analytical-locomotion.md`](../adr/0006-action-pose-layer-beside-analytical-locomotion.md) — why a tick-driven action layer sits beside distance-driven locomotion, and why ADR-0004 was not superseded
27. [`../adr/0007-procedural-impact-audio-boundary.md`](../adr/0007-procedural-impact-audio-boundary.md) — why the synth has no I/O, and where the device boundary is
28. [`../adr/0004-rigid-voxel-character-and-analytical-locomotion.md`](../adr/0004-rigid-voxel-character-and-analytical-locomotion.md) — why body parts are rigid and locomotion is analytical, and what that costs
29. [`../KNOWN_ISSUES.md`](../KNOWN_ISSUES.md) — every accepted limitation and every closed one, with the reason each was closed
30. [`../LEARNINGS.md`](../LEARNINGS.md) — reusable discoveries below ADR scope
31. [`EVIDENCE_HARNESS.md`](EVIDENCE_HARNESS.md) — how a capture is produced, what makes one invalid, and how it picks the right window
32. [`WORKFLOW.md`](WORKFLOW.md) — how an agent is expected to work in this repository

Then, as needed: [`../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md`](../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md), the rest of the [ADRs](../adr/README.md), and [`../environment/SETUP.md`](../environment/SETUP.md) for gate commands and environment variables.

Then the code. Read it in this order, because it is the surface anything after M7 works against:

- `crates/combat/src/movement.rs` — **the whole of movement legality**, in one place. `check_move` returns `MoveBlockReason`, `accepts` wraps it, `try_move` slides per axis, `separate` pushes two bodies apart through the same rules, and `TraversalLegality` is the veto a world hands in. Read this before anything that moves a body.
- `apps/client/src/traversal.rs` — the region reasoned about as columns: the water veto, the cached `SurfaceGrid`, the whole-region reachability audit that calls `check_move`, the derived and locked named route, the derived adversary placement, and the tests that judge all of it against the generated voxels rather than against each other.
- `crates/combat/src/adversary.rs` and `crates/combat/src/oracle.rs` — on the combat-initiative branch, the brain's two additions (the lunge choice and the owed spacing dodge) and the three policies that judge them. Read the brain's `decide` with one question: does anything here read what the player is *doing*? The answer must stay no.
- `apps/client/src/initiative.rs` — the same policies over the golden world's real ground and walkability, the open clearing where the opt-in session stands, and the spire witnesses that show where the capability breaks (KI-041).
- `crates/combat` — the fixed-step headless domain. One call to `Encounter::step` is one tick; every duration is a tick count; the weapon, the two attack specs, the swept hit query, the torso-column hurt volume, the kinematic movement rules, the adversary's state machine and the bounded event type all live here. It has no GPU, window, audio or filesystem types, and it depends on `crates/character` and never the reverse.
- `crates/character` — the descriptor, compiler, skeleton, distance-driven locomotion, ground contact, IK and collision representation, plus the tick-driven action pose layer M6 added beside it. `pose()` is M5's path and is unchanged; `pose_with()` takes an action overlay.
- `apps/client/src/encounter.rs` — where the domain meets the frame: the integer tick accumulator, the encounter modes including the frozen named moments, and the bounded per-frame event buffer.
- `apps/client/src/arena.rs` — where in the world a fight happens, and how a named camera pose is placed against the two bodies rather than against the map.
- `apps/client/src/input.rs` and `apps/client/src/camera.rs` — **both camera models live here.** With the encounter off the free-fly camera and raw keyboard state are exactly what they were before M6. With an encounter armed, input is latched on the key-down edge and interpreted relative to a third-person follow camera, and `HitShake` moves that camera on a confirmed hit and on nothing else.
- `apps/client/src/vfx.rs`, `readout.rs`, `synth.rs`, `audio.rs` — presentation that is **driven by `CombatEvent` and never by a guess**: chips and a telegraph accent from one fixed pool, the eight-pip health readout, the pure synth with no I/O, and the one cpal device behind a lock-free queue.
- `apps/client/src/renderer.rs` and the WGSL beside it — the world, shadow, character and effect passes, and the one shared lighting function terrain and bodies both call. Actors and weapons upload once; a frame writes transforms.
- `crates/procedural`, `crates/streaming` and `apps/client/src/world.rs` — how terrain is generated, made resident, and reaches the client. Combat touches this only through the `GroundSampler` trait.

The shapes worth holding while reading: the domain is authoritative and headless, the client advances it in whole ticks, and every visible or audible consequence of a fight is derived from an event the rules published.

## Continue here — independent QA of combat initiative

**The owner gate is closed: OWNER PLAYTEST — PRESSURE DEMANDS RESPONSE: PASS, 2026-09-23.** The single next step is an **independent QA of the whole branch** — every commit in `main...HEAD`, code, tests and documents — and it is the owner's to start. Until it has run: no pull request, no merge, no retune, no milestone number.

**What the owner judged, and only that.** The owner played `VELDWAKE_ENCOUNTER=initiative` on the implementation head `fea74017cfdf97aa1cbb808ba24605282e7ed71d`, several fights, told only *"lute normalmente"*, knowing the concept beforehand — so the session was **not blind**. Unprompted: *"achei da hora o combate agora, foi muito divertido, gostei."* Asked directly: just running in and attacking does not work; the attack is perceived before it lands; they started dodging and choosing when to go in with the dash in order to win; the missed lunge is perceived; the backstep looks natural; once understood it can be predicted, but it is fun. The exact words, with translations, are in [`../planning/COMBAT_INITIATIVE.md`](../planning/COMBAT_INITIATIVE.md#result--2026-09-23).

**What the owner did not say**, and no document may say for them: that combat is final, that every enemy should use this, that the tuning is definitive, that the lunge is visually perfect, that KI-041 or KI-042 is resolved, that M9 passed, that the found weapon was tested — the owner asked for the great sword, and it was correctly absent — or that the game's combat is solved.

**What independent QA should attack first**, because these are the places the branch decided something rather than derived it:

- **COMBAT-005.** Look for any input to `AdversaryBrain::decide` that depends on the player's action rather than its position. Then try to make `dodges_during_unresolved_swing` non-zero under the authored tuning — a player that swings on the first tick it is free is the obvious attempt — because non-reactivity is held by timing, not by a check.
- **The historical contract.** Re-run `combat-probe signature` and every M5/M7/M8 lock, and look for a historical path that reaches the pressure spec, `total_for(true)` with a different answer, or the new counters in the encounter trace.
- **The band and the no-bluff claim.** Re-derive them with `measure_the_lunge_band` rather than trusting the table, including the advancing escapes the fairness test now asserts.
- **The oracles themselves.** Are they kinder to spam or to read than a person is? Owner-spam's misjudgement range, spam-read's single rule and the read policy's lag handling decide every flat-ground number.
- **The lunge's aim at commit.** It reuses the swing-start aim assist with the band's far edge as its range; check that it never turns a body further than the M6 primary already does.
- **The terrain evidence and KI-041.** Confirm the clearing is open for the whole fight, and reproduce the two spire witnesses.
- **Every number in the slice document**, from the tests, the ignored measurements, the probe or a capture. The real-client harness is not in the repository; [`EVIDENCE_HARNESS.md`](EVIDENCE_HARNESS.md) says how to rebuild it.

**Branch state.** Read it with `git rev-parse HEAD` and `git rev-list --count main..HEAD`; a head copied into this file is stale within a day. The branch is pushed to `origin/feat/combat-initiative-spacing`, has no pull request, and must not be merged by an agent.

### M8's known weak points, still true in `main`

Three landmarks around the overlook at `(-69, 49)`: a spire 62 units west, a gate 63 units east, `159.7` degrees apart, and a broken monolith hidden from the overlook that shows itself from the gate at 149 units. The adversary stands at the spire, five columns from its crown, because the spire is the first choice that does not reveal the third. The known weak points:

- the crown of a first choice is **above the default camera frame** at 62 units (KI-032) — a deliberate, captured choice against a complete-but-tiny silhouette at 96 units. The owner read the gate as a broken construction rather than as a gate, which is consistent with its lintel being off-frame, though the owner said nothing about why;
- the overlook's `r = 40` vegetation reservation is a composition control nobody has judged on sight yet;
- the reveal is the weakest link: the forest leaves only two candidate sites visible from a landmark at all;
- the family still reads close to familiar voxel-RPG architecture (KI-036), which is art direction for a later milestone and not a defect in this one.

### What M7 left, and what it still means


**M7 produced new facts that should change how that proposal is reasoned about.** They are the point of writing them down here:

- **Traversal is enjoyable enough at this scope**, judged by a person, and running was deliberately not added. `63.9` seconds of walking was not experienced as dead time.
- **Locomotion reads as stiff and raw.** The owner's words, recorded as an observation and not acted on. Nothing about walk speed, turn rate, gait, acceleration, animation or the camera was changed in response, and nothing should be until a milestone takes movement feel as its subject.
- **Nothing in the world points at anything.** One adversary, `800 x 800` columns, no minimap, compass, marker, waypoint or landmark — and the owner walked the region and never found it (KI-029). Any milestone that places a second thing worth finding has to decide about discovery first.
- **The region is not one place.** Under the accepted movement bounds it splits into two large components, `314,861` and `307,144` standable columns, and a session begins in the **smaller** one (KI-027). Barrier attribution: water `2,464`, step-up `44,495`, max-drop `1,273`.
- **Water blocks and says nothing.** No wading, no splash, no shoreline cue; the body stops dead and slides along the shore (KI-028).
- **A teleport is a streaming discontinuity.** A player-defeat reset moves the anchor `164.5` units in one tick and the far field takes about `30` s to resolve (KI-030). Anything with fast travel, a second encounter elsewhere, or a load between places inherits this.
- **The runtime samples continuously; every audited claim is column-shaped.** Exact at column centres, up to `2` voxels apart away from them, and the water veto disagrees with itself in a one-column band along the waterline (KI-031). This is why the reachability audit is an upper bound.

To play what exists:

```text
VELDWAKE_ENCOUNTER=traverse cargo run --release -p veldwake-client
```

The world takes about a minute to settle — `time_to_idle_ms` was measured at `63,032`. The body starts at the route start `(-69, 49)`; the adversary stands `162` steps away at `(14, 191)`, dormant until you come within `14` world units of it. `WASD` walks relative to the camera, the mouse looks, `J` or left mouse attacks, `K` or `Space` dodges once the adversary is awake, `F4` detaches the camera. The named route is `217.09` world units and `63.9` seconds of walking.

### What is proven, and by whom

| claim | evidence | whose |
|---|---|---|
| the route is walkable | `the_route_is_walkable_in_a_real_encounter` drives a real encounter tick by tick over the real adapters; driven client sessions walked it repeatedly | tests and QA |
| the runtime accepts every step of it | `the_runtime_accepts_every_step_of_the_route_it_walks_continuously` puts thousands of walk-speed samples to `check_move` with the production adapters; none refused | tests |
| water blocks, on the drawn waterline | a driven session walked into the river and came to rest at `z = 24.81836700439453`; `slid_moves_player` `0` to `2,607` with `blocked_moves_player` at `0` | QA, real client |
| the adversary sleeps, then wakes | 10,000 ticks at 200 units leave it bit-identical with no stream draw; driven sessions woke it by walking up to it | tests and QA |
| **walking the world is worth doing** | played and judged | **the owner** |
| **exploration reaches the encounter** | walked the route, woke the adversary at `distance = 1.85` | QA, real client — **not the owner** |
| **player victory leaves the adversary down and the session going** | `adversary_health = 0`, `resets` `0` before and `0` after the hold, the body at bit-identical coordinates, the player `24.8` units away and still playing; eight captures opened | QA, real client — **not the owner** |
| **player defeat resets both bodies and the session continues** | `defeats_player = 1`, `resets` `0` to `1`, player `0.000` from its configured start, adversary `0.000` from the goal, then `9.2` units walked under input | QA, real client — **not the owner** |

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

## History — what M6 left behind, kept because its answers still bind

This section is the pre-M7 continuation point, preserved. Its *instructions* are spent — M7 was proposed, built, played and merged — but its facts and its answered questions are still the ground anything new stands on.

### What now exists

The repository is a playable vertical slice of one walk and one fight in one deterministic region:

- A finite 800 x 96 x 800 voxel region generated from a seed, streamed around the camera, lit with a sun, a shadow map, a procedural sky, fog and two weather states.
- A humanoid compiled from a descriptor: sixteen rigid voxel parts on a sixteen-bone skeleton, analytical gaits driven by distance travelled, soles solved against the ground with two-bone IK.
- A fight: two combatants, one procedurally generated sword, an adversary that telegraphs and commits, a swing decided by sweeping the blade against a torso-column volume, damage, stagger, knockback, hitstop, a camera that moves only on a confirmed hit, voxel chips, a diegetic health readout, and synthesised impact audio through one cpal device.
- A traversal: the player's body walks the whole region with streaming anchored on it, water blocks movement, an adversary placed by a whole-region reachability audit sleeps until proximity, and the session survives both outcomes of the fight.
- Evidence machinery: named camera poses, named frozen combat moments with tick offsets, five headless probes, locked fingerprints and behavioural signatures across five crates, a locked route signature, and a written capture harness.

### What still does not exist

No persistence of any kind — no saves, no world edits, no encounter state that survives a run. No second enemy, weapon, archetype or biome beyond what M5, M6 and M7 build. No inventory, stats, progression, quests or economy. No menu, HUD framework, pause or death screen. No networking, no multiplayer, no mod runtime, no editor. No physics engine and no collision simulation — movement is kinematic against a ground query. No aggregate world simulation, settlements or history. No running, jumping, climbing or swimming, and no fast travel. No discovery affordance of any kind — no minimap, compass, marker or waypoint. No music, reverb or mixer. No automated visual regression, and one host and one adapter for every visual claim in the repository (KI-006, KI-021).

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

None. Everything a session with no prior context needs is in the repository: the merged state and its SHAs, what each milestone built and what it deliberately did not, the crate boundaries and the test a new crate must pass, the invariants, the determinism rules, the measurement method with its host, the visual contracts, the evidence harness with its traps, and every accepted limitation as a numbered open issue rather than as prose.

Nothing material about the project's real state exists only in a conversation. The decisions that outlive M7 are in [ADR-0008](../adr/0008-traversal-legality-separate-from-ground-contact.md) and in the two invariants it carries, **MOVE-001** and **TRAVERSE-001**, both in [`../engineering/INVARIANTS.md`](../engineering/INVARIANTS.md); the ones that do not are in [`../planning/M7_TRAVERSABLE_REGION.md`](../planning/M7_TRAVERSABLE_REGION.md) beside the capture or the measurement that produced them, including the whole branch-QA section. M6's are in [ADR-0005](../adr/0005-fixed-step-headless-combat-domain.md), [ADR-0006](../adr/0006-action-pose-layer-beside-analytical-locomotion.md) and [ADR-0007](../adr/0007-procedural-impact-audio-boundary.md).

## Immediate risks

- **Combat initiative has its owner gate and not its QA.** OWNER PLAYTEST — PRESSURE DEMANDS RESPONSE: PASS, 2026-09-23. Do not open a pull request, merge, retune, give it a milestone number, or start other work on top of it until an independent pass over `main...HEAD` has run. The owner judged that pressure demands a response; every number in its document is still headless or real-client self-QA evidence, and none becomes owner judgement by association.
- **Do not act on the owner's remarks as if they were work items.** *"Dá pra prever"* — predictable once understood, and still fun — is recorded as an observation next to the risk that spam-read wins cleanly. The request for the great sword belongs to M9, which is frozen on its own branch and is not part of this gate.
- **Do not re-lock a historical value for this capability.** `GOLDEN_ENCOUNTER_SIGNATURE`, both weapon fingerprints and every M5/M7/M8 lock stayed byte-identical and must stay so; if one moves, the capability has leaked into the historical encounter. `COMBAT_INITIATIVE_SIGNATURE` is the lock that is allowed to move, with an OLD/NEW/WHY paragraph.
- **The spacing dodge must stay non-reactive.** Its trigger is the adversary's own state — its stagger ended, or its own lunge connected — and `dodges_during_unresolved_swing` must stay `0` for the adversary. Adding the player's action to the brain's inputs turns this into `feat/combat-pressure`, which is blocked for a measured reason.
- **Stone behind the adversary turns the capability off** (KI-041). The opt-in session stands in the open clearing on purpose; the M8 encounter at the spire does not use the capability, and moving it there is a navigation decision.
- **`combat-shoulder` frames from behind the adversary** (KI-042). Do not use it as evidence of what the player sees.
- **The golden-world initiative gates are slow in debug** — the open-clearing gate took `80` s and the spire gate `139` s under `cargo nextest` on the audited host, because each builds a world and plays dozens of sixty-second fights. They are not ignored; they are the terrain evidence.
- **The owner judged M8's discovery and nothing else.** Two destinations noticed, read as different, one chosen, walked to, adversary found. Distances, contrast, the third landmark, the gate's passability, the weather comparison and KI-032 are all self-QA evidence. Promoting any of them to owner judgement is the M7 mistake repeated.
- **The visual-simplicity and Cube-World observations are recorded, not actioned.** The owner said the structures are simple but sufficient, and that they still read close to familiar voxel-RPG architecture (KI-036). Neither is a work item on this branch: no props, no decoration, no second family, no second material system, no particles, no lights, no interiors.
- **The landmark plan is derived when a world is built, not when a chunk is generated.** It costs `248-272 ms` in release and `1,108 ms` in debug per world. `cargo nextest` runs one process per test, so no cache helps the suite; what helps is building one world per process, which is what `WorldSelection::build` and `TerrainGenerator::with_plan` are for. Do not reintroduce a lazy or global plan to make a number look better.
- **`LANDMARK_BEHAVIOR_SIGNATURE` is the golden plan's own fingerprint** and is folded into the world fingerprint. Any change to placement, the compiler, the descriptor or the controls moves it, invalidates every cached chunk, and needs an OLD/NEW/WHY paragraph — as do `TERRAIN_BEHAVIOR_SIGNATURE`, `GOLDEN_REGION_SIGNATURE`, `GOLDEN_ROUTE_SIGNATURE` and `ADVERSARY_COLUMN`, all of which M8 moved once.
- **`TERRAIN_GENERATOR_VERSION` must not be bumped for landmark work.** `WorldSeed::stream` folds it into every stream, so bumping it reseeds the valley and regenerates a world nobody asked to change.
- **Visibility has two levels and neither is the whole answer.** The world proxy in `veldwake-procedural` answers occlusion and knows nothing about cameras; the oracle in `apps/client/src/landmark.rs` answers framing and legibility and does no raycasting. A landmark is visible when both agree, and `the_world_proxy_and_the_presentation_agree_about_what_is_visible` is what keeps the placement honest.
- **A landmark keep-out is a refusal, never a surface.** `GroundSampler` is untouched, nothing walks on a landmark, and the keep-out is the widest body's capsule — `0.86` by `2.84`, measured from the compiled rigs — applied to a destination only, exactly like the water veto.
- **There is no active milestone after M8, and that will again be the state, not an omission.** Propose before coding. Do not name a milestone, do not create a `feat/*` branch, and do not treat the roadmap's capability groups as a queue. The last three milestones were all proposed from the state of the repository rather than from the order of that list.
- **Owner evidence and QA evidence are different things and M7 is where they diverge.** The owner judged the traversal and never reached the encounter. Every claim about exploration reaching combat, about a victory, and about the session continuing past one is branch-QA evidence from the real client. Do not promote it.
- **A smoothed presentation value must never decide an authoritative rule** (MOVE-001). `base_height` is the filtered pelvis height; at `tau = 0.12 s` a body climbing at gradient `g` carries about `0.42 g` of lag, and with `max_step_up` exactly one voxel *any* lag makes a legal step illegal.
- **`movement::check_move` is the only implementation of movement legality**, it returns `MoveBlockReason`, and the reachability audit calls it. Do not write a second copy to answer "why did that fail".
- **`GroundSampler` is unchanged and must stay unchanged.** It answers where the visible solid surface is, including the river bed inside a river. Whether a body may walk there is `TraversalLegality`, a veto consulted about a move's destination only (ADR-0008).
- **Water is the voxel predicate, not the continuous relation** (TRAVERSE-001). `has_water_voxel()` is `water_surface_y() > surface_y()`, exactly what `fill_terrain` writes; `is_submerged()` is a different question and blocks columns a viewer sees as dry.
- **Column-shaped claims are exact only at column centres** (KI-031), because `TerrainGround` and `TerrainWalkability` sample the terrain field continuously. Three tests pin the size of the gap. Do not column-quantise either adapter to tidy this up — it would move the M5 and M6 locked signatures.
- **Do not re-lock `GOLDEN_ROUTE_SIGNATURE` to make a test pass.** It covers the world identity, the compiled movement spec, `TRAVERSAL_RULE_VERSION`, both endpoints, every waypoint and every checkpoint, so a changed `max_step_up` moves it even when the path does not. Every re-lock carries an OLD/NEW/WHY paragraph.
- **`ADVERSARY_COLUMN` is a cache of a derivation, not a hand-picked pair.** `the_derived_placement_is_the_locked_one` is `#[ignore]`d because the search costs about fourteen seconds; run it deliberately after anything that touches the terrain, the movement spec or the placement rules.
- **No test asserts a duration, and none should.** The region sample measured `390` and `590` ms on the same host in different runs with identical answers; the audit `160` and `273` ms. Durations are observations on a named host.
- **The traversal dodge gate must stay inside the tick loop.** A frame runs up to four ticks and the first can cross the aggro radius; hoisting the brain-state read would make a dodge depend on how the frames were cut, which is the one thing the integer clock exists to prevent.
- **`CombatCounters` is not part of the encounter trace.** That is why `slid_moves` could be added without moving `GOLDEN_ENCOUNTER_SIGNATURE`. Anything that goes into `run_script`'s per-tick bytes *does* move it.
- **Evidence about anything shorter than five seconds cannot come from the log.** `combat state` is published on a five-second cadence, so a defeat, its `2.5` s hold and the encounter reset all fit between two reports. Capture continuously and read the frames. The cadence was left alone deliberately.
- **A camera on ground that descends behind the body looks into the bank** (KI-026), and the body can leave the frame entirely. A clamp was written, measured at `0.05` world units of correction, and reverted rather than kept as a fix that fixes nothing.
- **One host, one adapter, no automated image comparison** (KI-006, KI-021). Every visual claim in this repository rests on captures taken by a person or an agent and read in writing.
