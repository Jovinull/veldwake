# Current handoff

Last updated: 2026-09-23

## Current position

- **M8 — Discoverable Landmarks**: merged.
- **Combat Initiative / Spacing**: merged and owner-tested.
- **M9 — Meaningful Reward**: the original stays frozen on `feat/m9-meaningful-reward` (technical PASS, owner FAIL 2026-09-22). The **M9 revisit** on `feat/m9-meaningful-reward-revisit` ported it onto combat initiative (port complete, headless pre-gate PASS) and **failed its owner gate on 2026-09-23** — the found weapon was perceived as better overall, the original as offering no practical advantage — while combat initiative held with either weapon. After a causal investigation the owner **approved one retune** (found damage `32` → `28`, windup `31` → `36` ticks), now implemented and re-measured: **ready for `OWNER PLAYTEST — WEAPON CHOICE MATTERS (REVISIT 2)`, not yet run.** Unmerged, no pull request, no independent QA; M9 is not complete.

**The found weapon was retuned once, by owner decision, and waits for REVISIT 2.** Damage `32` → `28` and windup `0.26` s → `0.30` s (`31` → `36` ticks); nothing else about either weapon, the adversary or combat initiative changed. `FOUND_ENCOUNTER_SIGNATURE` (`0xa5b7_8ee1_c589_a499`) and `REWARD_BEHAVIOR_SIGNATURE` (`0x08ac_216e_2ef0_0962`) were re-locked with OLD/NEW/WHY; every other lock, `COMBAT_INITIATIVE_SIGNATURE` included, is exact. Re-measured with the same policies: the found weapon keeps its reach and its contact from where a person swings (mashing at `3.0` u: found `6/0`, original `0/6`) and now pays for a bad commit (spam-read at `400` ms: found `3/3` at `144` of `576` health, original `6/0` at full); owner-spam is `2/4` for both; the original is faster close in and in a quick clean read. All headless and self-QA evidence; see [`planning/M9_MEANINGFUL_REWARD.md`](../planning/M9_MEANINGFUL_REWARD.md#approved-retune-after-revisit-fail).

**The M9 revisit failed its owner playtest on 2026-09-23, and combat initiative passed again inside it.** `OWNER PLAYTEST — WEAPON CHOICE MATTERS (REVISIT): FAIL`: in the weapon-choice laboratory the owner fought differently with the two weapons — further away and at different moments with the found one — but judged the found weapon better overall (*"a espada maior parecia no geral melhor"*, verbatim in the milestone document), named no situation that favoured the original, and felt *more* danger with the original. Asked whether running in and attacking worked again with the found weapon, the owner said *"não"*: the FAIL is about the sidegrade's balance and affordance, not a combat initiative regression. No retune is authorised. Before that session: `feat/m9-meaningful-reward-revisit`, cut from `docs/post-combat-initiative-handoff` at `0aeac9c849a27e71d249d5d013376d50e1896e9d`, is a fresh port of the frozen M9 onto combat initiative — no rebase, merge or cherry-pick; the old branch is untouched at `61cb43e76af9f307bb388cb1bdebff84e4425b2b`. The found weapon is the frozen branch's exactly, every M9 and historical lock is exact, `COMBAT_INITIATIVE_SIGNATURE` did not move, and the adversary provably decides the same whichever weapon the player holds (COMBAT-005, ARM-002). The headless pre-gate measured both weapons against the lunge and passed every hard stop: found owner-spam `3/3` against the original's `2/4`, the lunge cut by the found weapon in `10` of the band's `77` cells and in `0` of `24` fight lunges, the original more forgiving after a late read (spam-read at `400` ms keeps full health with it and loses `252` of `576` with the found weapon), the found weapon a quarter to a third faster for a clean reader at equal safety. The owner-facing session is a laboratory — `VELDWAKE_ENCOUNTER=weapon-choice`, combat initiative's clearing with the exchange beside the round start and every round starting paused — not the product world, where the spire still fights without initiative (KI-043). All of it is headless and self-QA evidence. See [`planning/M9_MEANINGFUL_REWARD.md`](../planning/M9_MEANINGFUL_REWARD.md#m9-revisit).

**Combat Initiative / Spacing is complete and merged.** Implementation complete; author self-QA complete; **OWNER PLAYTEST — PRESSURE DEMANDS RESPONSE: PASS**, 2026-09-23; **independent QA: PASS**, 2026-09-23. It landed through [PR #13](https://github.com/Jovinull/veldwake/pull/13) at merge commit `af475efc18139dfc4b86b3c41e165dcfd0d7393c`, whose parents are `ee35f62f97afbe3d001a27a576e9bae21e77c4d2` and `3c0c540f4ac486b7a8b45316da1b531835f65960`, after author self-QA, the owner's playtest (OWNER PLAYTEST — PRESSURE DEMANDS RESPONSE: PASS, 2026-09-23), independent QA (PASS, 2026-09-23), a green pull-request CI run ([run 35873074352](https://github.com/Jovinull/veldwake/actions/runs/35873074352)) and a green post-merge CI run on the merge commit ([run 35875711984](https://github.com/Jovinull/veldwake/actions/runs/35875711984)). The remote branch `feat/combat-initiative-spacing` is preserved at `3c0c540f4ac486b7a8b45316da1b531835f65960`, the head where the 793 tests and every gate were run. It has no milestone number — the owner approved it as an unnumbered slice. Read [`../planning/COMBAT_INITIATIVE.md`](../planning/COMBAT_INITIATIVE.md) before touching it: the three mechanisms, every number and the measurement that moved it, the oracles, the terrain results, the real-client evidence table, and what this agent could not verify.

**The one thing to hold:** the capability exists only when a tuning authors `adversary_pressure`. The historical encounter, every fixture, script and lock is byte-identical — `GOLDEN_ENCOUNTER_SIGNATURE` is still `0x6415_7522_d253_5658` — and the new behaviour is locked separately at `COMBAT_INITIATIVE_SIGNATURE = 0x8238_2662_d859_8cf3`. The two are never compared.

**Combat initiative is not "combat final".** The owner did not say that combat is final, that every enemy should use this, that the tuning is definitive or that the lunge is visually finished, and KI-041 and KI-042 remain open.

**The original M9 is frozen and stays so.** `feat/m9-meaningful-reward` failed its product gate on 2026-09-22 because approaching and attacking won with either weapon; combat initiative is what that gate lacked, and the revisit branch above is where M9 now continues. Nothing touches the frozen branch. `feat/combat-pressure` stays blocked for its measured reason: combat initiative answers the same finding by acting **before** the player commits, where that branch reacted after.

**Combat initiative's pull request also carried the post-M8 handoff.** `docs/post-m8-handoff` (`658ebfbd618b1d7387eee7890d11af53b5f8e045`) was merged into `feat/combat-initiative-spacing` with every conflict resolved by hand, so the M8 record below reached `main` in one narrative, and `feat/m9-meaningful-reward`, which was built on that commit, now shares its base with `main`. That branch is preserved and needs no separate merge.

**M8 — Discoverable Landmarks is complete and merged. M1 through M8 are all in `main`.** It landed through [PR #12](https://github.com/Jovinull/veldwake/pull/12) at merge commit `ee35f62f97afbe3d001a27a576e9bae21e77c4d2`, whose parents are `0c81c069bb95a8caf3b6a5252b89b023c6334ae1` and `d5e200bbd18d7c5aee167151509b89be260b85e2`, after the owner's discovery playtest, independent branch QA, a green pull-request CI run ([run 35732875867](https://github.com/Jovinull/veldwake/actions/runs/35732875867)) and a green post-merge CI run on the merge commit ([run 35735467195](https://github.com/Jovinull/veldwake/actions/runs/35735467195)). The remote branch `feat/m8-discoverable-landmarks` is preserved at `d5e200bbd18d7c5aee167151509b89be260b85e2`, the head where the 755 tests and every gate were run. The workspace has **755 tests**, three of them `#[ignore]`d; all **758** pass when the ignored ones are run explicitly.

**What M8 proved, and it is a product result rather than a technical one.** M7: walking the world worked, and the owner found nothing in it. M8: the owner noticed two different destinations without any navigation instruction, chose one, explored toward it, and found the adversary. That is the change the milestone existed to make.

**OWNER DISCOVERY PLAYTEST WITHOUT NAVIGATION INSTRUCTIONS: PASS**, 2026-09-22. The wording is exact and the exactness matters. During the session the owner received no coordinate, no direction, no waypoint, no route and no navigation UI; the owner had read the implementation report beforehand, so the session was *uninstructed* rather than literally blind. The owner noticed two structures naturally, perceived them as two different destinations, read one as a tower and the other as a broken construction or ruin, chose the tower deliberately, walked to it, **found the adversary while exploring**, and liked the result. The structures were judged simple but good and sufficient for the current objective, with the observation that they still read somewhat close to the visual language of Cube World and familiar voxel RPGs (KI-036).

**What M8 did not prove.** The owner gave no distance measurement, no contrast judgement, no judgement of the third landmark, no clear-versus-overcast comparison, no judgement of the gate's mechanical passage and no judgement of KI-032 — all of those are technical and QA evidence and must never be promoted to owner judgement. Nothing was shown about whether the architecture has an identity of its own, whether the overlook clearing reads as natural, or anything at all about persistence, progression, a second kind of content or a world beyond this region. The three landmarks are stones with no history behind them.

**Independent branch QA reviewed the whole branch and passed it**, after finding and fixing one real defect: `CompiledMonolith::new` trusted an informal precondition that its descriptor had been validated, so `band_period = 0` could panic during compilation and negative dimensions could silently compile an empty landmark. It now validates and returns `DescriptorError`, and `PlanError::Descriptor` propagates it. QA also added eight oracles that judge the landmarks from the voxels the generator draws rather than from the plan — no terrain or water overwrite, chunk-order independence, drawn solids against traversal occupancy, a body walked through the gate and stopped by a pillar in the authoritative loop, reservation suppression against raw voxels, no landmark on a water margin, a valid overlook in the finished world, and all thirteen landmark controls moving the world fingerprint — re-derived every number the milestone claims, and corrected three of them.

Read [`../planning/M8_DISCOVERABLE_LANDMARKS.md`](../planning/M8_DISCOVERABLE_LANDMARKS.md) before touching anything M8 built. Its durable decision is [ADR-0009](../adr/0009-two-level-landmark-visibility-and-eager-world-plan.md) — two-level visibility and an eagerly derived world plan — and its two invariants are **LAND-001** (a landmark is written, never carved) and **LAND-002** (a visible wall refuses a body, and an opening does not), both in [`../engineering/INVARIANTS.md`](../engineering/INVARIANTS.md).

**M7 — Traversable Region is complete and merged.** It landed through [PR #11](https://github.com/Jovinull/veldwake/pull/11) at merge commit `0c81c069bb95a8caf3b6a5252b89b023c6334ae1`, whose parents are `f840ff7880e1857e86b3a74c4d3f66ceaf82a922` and `2a747d859fe03db4a84e6f57d32035dd8e1feb4a`, after the owner's playtest, independent branch QA, a green pull-request CI run ([run 35630288272](https://github.com/Jovinull/veldwake/actions/runs/35630288272)) and a green post-merge CI run on the merge commit ([run 35632133251](https://github.com/Jovinull/veldwake/actions/runs/35632133251)). The remote branch `feat/m7-traversable-region` is preserved at `2a747d859fe03db4a84e6f57d32035dd8e1feb4a`, which is the head where the 699 tests and every gate were run. The branch was based on `docs/post-m6-handoff` at `aac3ec55519d93e44397af549eb18089c7dcdfcd` deliberately, so the two post-M6 documentation commits travelled into the same pull request.

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
13. [`../procedural/CONTENT_DOMAINS.md`](../procedural/CONTENT_DOMAINS.md) — each procedural content domain, and which parts of it are real
14. [`../procedural/WORLD_GENERATION.md`](../procedural/WORLD_GENERATION.md) — the proposed world pipeline and how little of it M4 implemented
15. [`../audiovisual/ART_DIRECTION.md`](../audiovisual/ART_DIRECTION.md) — the aesthetic direction
16. [`../audiovisual/STYLE_BIBLE.md`](../audiovisual/STYLE_BIBLE.md) — the versioned, checkable constraints M4 was held to, and what they explicitly do not cover
17. [`../audiovisual/CHARACTER_STYLE.md`](../audiovisual/CHARACTER_STYLE.md) — the versioned, checkable constraints M5 was held to, and how they relate to the style bible
18. [`../audiovisual/LANDMARK_STYLE.md`](../audiovisual/LANDMARK_STYLE.md) — the versioned, checkable constraints M8's one landmark family is held to, and what the implementation moved
19. [`../audiovisual/COMBAT_STYLE.md`](../audiovisual/COMBAT_STYLE.md) — the versioned, checkable constraints M6 was held to: the weapon, the six action poses (the lunge is the sixth), the two effects, the readout, and what the camera may do
20. [`../design/COMBAT.md`](../design/COMBAT.md) — the design intent combat aims at, most of which is still not built
21. [`../planning/M9_MEANINGFUL_REWARD.md`](../planning/M9_MEANINGFUL_REWARD.md) — M9: the original record and its owner FAIL, unchanged, and the M9 revisit at the end — the port, the pre-gate tables, the laboratory and the prepared owner protocol; with [ADR-0011](../adr/0011-session-acquired-state-in-the-authoritative-encounter.md) and ARM-001/ARM-002
22. [`../planning/COMBAT_INITIATIVE.md`](../planning/COMBAT_INITIATIVE.md) — the most recent work, an unnumbered slice: the three mechanisms, every number, the owner's gate and independent QA; and [ADR-0010](../adr/0010-sixth-action-keeps-the-keyed-layer.md), why its lunge is still a keyed curve
23. [`../planning/M8_DISCOVERABLE_LANDMARKS.md`](../planning/M8_DISCOVERABLE_LANDMARKS.md) — the most recent numbered milestone: the composition, the owner's session, the branch QA section, and what nobody could verify
24. [`../planning/M7_TRAVERSABLE_REGION.md`](../planning/M7_TRAVERSABLE_REGION.md) — the milestone before it: the owner decisions it answers to, the two architectural corrections it carries, the whole-region reachability result, the named route and everything measured
25. [`../planning/M6_COMBAT_SLICE.md`](../planning/M6_COMBAT_SLICE.md) — the milestone before it: its evidence, its measurements, the six branch-QA findings, the owner gates, and its limitations
26. [`../planning/M5_PROCEDURAL_CHARACTER.md`](../planning/M5_PROCEDURAL_CHARACTER.md) — the milestone before it, and the character every combat pose is built on
27. [`../adr/0009-two-level-landmark-visibility-and-eager-world-plan.md`](../adr/0009-two-level-landmark-visibility-and-eager-world-plan.md) — why landmark visibility is answered at two levels and why the world plan is derived eagerly and typed
28. [`../adr/0008-traversal-legality-separate-from-ground-contact.md`](../adr/0008-traversal-legality-separate-from-ground-contact.md) — why a traversal veto sits beside `GroundSampler` instead of changing it, and why the veto never reports a height
29. [`../adr/0005-fixed-step-headless-combat-domain.md`](../adr/0005-fixed-step-headless-combat-domain.md) — why combat is integer-stepped and headless, and why two combatants are the whole entity model
30. [`../adr/0006-action-pose-layer-beside-analytical-locomotion.md`](../adr/0006-action-pose-layer-beside-analytical-locomotion.md) — why a tick-driven action layer sits beside distance-driven locomotion, and why ADR-0004 was not superseded
31. [`../adr/0007-procedural-impact-audio-boundary.md`](../adr/0007-procedural-impact-audio-boundary.md) — why the synth has no I/O, and where the device boundary is
32. [`../adr/0004-rigid-voxel-character-and-analytical-locomotion.md`](../adr/0004-rigid-voxel-character-and-analytical-locomotion.md) — why body parts are rigid and locomotion is analytical, and what that costs
33. [`../KNOWN_ISSUES.md`](../KNOWN_ISSUES.md) — every accepted limitation and every closed one, with the reason each was closed
34. [`../LEARNINGS.md`](../LEARNINGS.md) — reusable discoveries below ADR scope
35. [`EVIDENCE_HARNESS.md`](EVIDENCE_HARNESS.md) — how a capture is produced, what makes one invalid, and how it picks the right window
36. [`WORKFLOW.md`](WORKFLOW.md) — how an agent is expected to work in this repository

Then, as needed: [`../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md`](../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md), the rest of the [ADRs](../adr/README.md), and [`../environment/SETUP.md`](../environment/SETUP.md) for gate commands and environment variables.

Then the code. Read it in this order, because it is the surface anything after M7 works against:

- `crates/combat/src/movement.rs` — **the whole of movement legality**, in one place. `check_move` returns `MoveBlockReason`, `accepts` wraps it, `try_move` slides per axis, `separate` pushes two bodies apart through the same rules, and `TraversalLegality` is the veto a world hands in. Read this before anything that moves a body.
- `apps/client/src/traversal.rs` — the region reasoned about as columns: the water veto, the cached `SurfaceGrid`, the whole-region reachability audit that calls `check_move`, the derived and locked named route, the derived adversary placement, and the tests that judge all of it against the generated voxels rather than against each other.
- `crates/combat/src/adversary.rs` and `crates/combat/src/oracle.rs` — combat initiative's two additions to the brain (the lunge choice and the owed spacing dodge) and the three policies that judge them. Read the brain's `decide` with one question: does anything here read what the player is *doing*? The answer must stay no.
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

## Continue here — REVISIT 2 of the M9 weapon-choice gate, by the owner

**The next step is the owner's: `OWNER PLAYTEST — WEAPON CHOICE MATTERS (REVISIT 2)`.** The first revisit failed on 2026-09-23 (verbatim in [`../planning/M9_MEANINGFUL_REWARD.md`](../planning/M9_MEANINGFUL_REWARD.md#result--2026-09-23)); the owner approved one retune of the found weapon — damage `28`, windup `36` ticks — and it is implemented, re-measured and self-QA'd (*APPROVED RETUNE AFTER REVISIT FAIL* in the same document).

```text
VELDWAKE_ENCOUNTER=weapon-choice cargo run --release -p veldwake-client
```

Same laboratory, same protocol, same nine questions. Do not tell the owner the retune before play; the instruction is only *"Lute normalmente. Entre as lutas, você pode trocar de arma no ponto de início quando quiser."* Record the owner's words verbatim in a new result subsection, beside — never over — both earlier FAILs.

**After the verdict, stop either way.** No second retune without owner approval, no independent QA, no pull request, no merge. On a PASS, return to the owner with the evidence and the open product-world step (KI-043). On a FAIL, record it, including what the owner did not say.

### Combat initiative — how it was closed

**OWNER PLAYTEST — PRESSURE DEMANDS RESPONSE: PASS**, 2026-09-23. **What the owner judged, and only that.** The owner played `VELDWAKE_ENCOUNTER=initiative` on the implementation head `fea74017cfdf97aa1cbb808ba24605282e7ed71d`, several fights, told only *"lute normalmente"*, knowing the concept beforehand — so the session was **not blind**. Unprompted: *"achei da hora o combate agora, foi muito divertido, gostei."* Asked directly: just running in and attacking does not work; the attack is perceived before it lands; they started dodging and choosing when to go in with the dash in order to win; the missed lunge is perceived; the backstep looks natural; once understood it can be predicted, but it is fun. The exact words, with translations, are in [`../planning/COMBAT_INITIATIVE.md`](../planning/COMBAT_INITIATIVE.md#result--2026-09-23).

**What the owner did not say**, and no document may say for them: that combat is final, that every enemy should use this, that the tuning is definitive, that the lunge is visually perfect, that KI-041 or KI-042 is resolved, that M9 passed, that the found weapon was tested — the owner asked for the great sword, and it was correctly absent — or that the game's combat is solved.

**Independent QA: PASS**, 2026-09-23 — a cold-start audit of the whole branch: `793/793` by default and `798/798` with `--run-ignored all`, every combat fingerprint and `COMBAT_INITIATIVE_SIGNATURE` matching in debug and release, the flat-ground and golden-terrain oracles, the structural non-reactivity tests, reset paths, frozen and live release-client sessions, and a clean alternating release A/B against `main`. Its only correction was the formatting of COMBAT-005 (`ea0d1d1`). It observed that at close follow-camera angles the player can partly occlude the adversary — a framing observation, not a judgement of legibility. The details are in [`../planning/COMBAT_INITIATIVE.md`](../planning/COMBAT_INITIATIVE.md#gates).

**What independent QA attacked, and where any later change to it should look first**, because these are the places the branch decided something rather than derived it:

- **COMBAT-005.** Look for any input to `AdversaryBrain::decide` that depends on the player's action rather than its position. Then try to make `dodges_during_unresolved_swing` non-zero under the authored tuning — a player that swings on the first tick it is free is the obvious attempt — because non-reactivity is held by timing, not by a check.
- **The historical contract.** Re-run `combat-probe signature` and every M5/M7/M8 lock, and look for a historical path that reaches the pressure spec, `total_for(true)` with a different answer, or the new counters in the encounter trace.
- **The band and the no-bluff claim.** Re-derive them with `measure_the_lunge_band` rather than trusting the table, including the advancing escapes the fairness test now asserts.
- **The oracles themselves.** Are they kinder to spam or to read than a person is? Owner-spam's misjudgement range, spam-read's single rule and the read policy's lag handling decide every flat-ground number.
- **The lunge's aim at commit.** It reuses the swing-start aim assist with the band's far edge as its range; check that it never turns a body further than the M6 primary already does.
- **The terrain evidence and KI-041.** Confirm the clearing is open for the whole fight, and reproduce the two spire witnesses.
- **Every number in the slice document**, from the tests, the ignored measurements, the probe or a capture. The real-client harness is not in the repository; [`EVIDENCE_HARNESS.md`](EVIDENCE_HARNESS.md) says how to rebuild it.

### What M8 produced, and what still holds

- **A direction can now mean something, and a person acted on it.** Told nothing, the owner saw two destinations, chose one and walked to it. Discovery through world content works at the smallest possible scale: three stones, one family, no interface.
- **The content is thin and the owner said so.** "Simple, but good and sufficient for the current objective", and still close to the visual language of familiar voxel RPGs (KI-036). That is an observation about art direction, not a work item, and it must not become a milestone by default.
- **There is no reason for anything to be anywhere.** The landmarks are composed to be seen and walked to. No history produced them, no culture built them, nothing else in the world refers to them. `docs/procedural/WORLD_GENERATION.md` calls this out: M8 occupies the "ruins" position in the candidate stage list without any of the history that is supposed to produce them.
- **One adversary is still the whole of what a player finds.** Walking to a landmark ends in a fight or in a reveal, and nothing else in the world responds to the player at all.
- **The world costs more to build than it did.** Deriving the landmark plan is `316`–`409` ms in release and about `1.5` s in debug per world (KI-035), paid eagerly, and `cargo nextest` pays it once per test process.
- **The camera met its first man-made occluder.** Walking close past a shaft puts it inside the stone (KI-025). Harmless for navigation today; a milestone that puts a player among structures inherits it.
- **Everything M7 recorded still stands**: the region splits into two components with the session in the smaller one (KI-027), water blocks with no cue (KI-028), a teleport is a thirty-second streaming discontinuity (KI-030), and every column-shaped claim is exact only at column centres (KI-031).

To play what exists:

```text
VELDWAKE_ENCOUNTER=traverse VELDWAKE_PROFILE=m4-golden cargo run --release -p veldwake-client
```

The session begins at the discovery overlook `(-69, 49)`. Two landmarks are visible from it about `160` degrees apart — a spire `62` units away and a gate `63` — and a third, a broken monolith, is hidden from there and shows itself from the gate at `149`. The adversary stands at the spire, five columns from its crown. `WASD` walks relative to the camera, the mouse looks, `J` or left mouse attacks, `K` or `Space` dodges once the adversary is awake, `F3` toggles the weather, `F4` detaches the camera.

To play combat initiative:

```text
VELDWAKE_ENCOUNTER=initiative cargo run --release -p veldwake-client
```

It starts paused at the open clearing `(-69, 49)` with the adversary eight units away, and arms on the first input.

To play the M9 revisit's weapon-choice laboratory (branch `feat/m9-meaningful-reward-revisit` only):

```text
VELDWAKE_ENCOUNTER=weapon-choice cargo run --release -p veldwake-client
```

The same clearing and adversary, the found weapon planted `1.25` u to the player's left, `E` to swap, and every round starting paused beside it.

### Where M8's code is, and in what order to read it

- `crates/procedural/src/landmark/` — `material.rs`, `descriptor.rs`, `compile.rs`, `visibility.rs`, `plan.rs`. Read `plan.rs` last: it is the composition, and the rest is what it composes.
- `crates/procedural/src/generator.rs` — where the plan is derived, and where landmark voxels are written into air after vegetation.
- `crates/procedural/src/vegetation.rs` — `WorldVegetation`, the one composed answer about plants.
- `apps/client/src/traversal.rs` — the keep-out, the surface grid, the audit, the route and the adversary's placement.
- `apps/client/src/landmark.rs` — the presentation oracle and the capture poses.
- `crates/procedural/src/bin/terrain-probe.rs` — `landmarks` prints the plan, re-derives it and refuses to print if the two disagree.

### What M8 proved, and by whom

| claim | evidence | whose |
|---|---|---|
| **two destinations are noticeable without an interface** | played and judged | **the owner** |
| **a person chooses one and walks to it** | played and judged | **the owner** |
| **exploration reaches the adversary** | played and judged | **the owner** |
| the landmarks are where the plan says, deterministically | `the_golden_composition_is_locked`, `the_plan_is_a_pure_function_of_the_world`, the probe's own re-derivation | tests |
| no landmark replaces terrain or water | `qa_a_landmark_only_ever_replaces_air`, differencing against a landmark-free world of the same identity | QA oracle |
| a visible wall refuses a body | `qa_the_keep_out_agrees_with_the_voxels_a_viewer_can_see`, the drawn solids against the veto | QA oracle |
| the gate's opening admits one | `qa_a_real_encounter_walks_a_body_through_the_gate_and_into_its_pillars`, the authoritative tick loop | QA oracle |
| the three landmarks are reachable on foot | the whole-region audit: `65`, `61` and `211` steps | tests, `#[ignore]`d report |
| it looks like something | eleven driven client runs, every capture opened and read | QA, real client — **not the owner** |

## History — what M7 left behind, kept because its answers still bind

**These were M7's new facts, and they remain true of the merged world.** M8 answered the third of them and left the rest standing:

- **Traversal is enjoyable enough at this scope**, judged by a person, and running was deliberately not added. `63.9` seconds of walking was not experienced as dead time.
- **Locomotion reads as stiff and raw.** The owner's words, recorded as an observation and not acted on. Nothing about walk speed, turn rate, gait, acceleration, animation or the camera was changed in response, and nothing should be until a milestone takes movement feel as its subject.
- **Nothing in the world points at anything** — the fact M8 was built to answer, and did. One adversary, `800 x 800` columns and no affordance of any kind; the owner walked the region and never found it. KI-029 is closed by the M8 session, and the affordance that closed it is world content rather than an interface.
- **The region is not one place.** Under the accepted movement bounds it splits into two large components, `314,861` and `307,144` standable columns, and a session begins in the **smaller** one (KI-027). Barrier attribution: water `2,464`, step-up `44,495`, max-drop `1,273`.
- **Water blocks and says nothing.** No wading, no splash, no shoreline cue; the body stops dead and slides along the shore (KI-028).
- **A teleport is a streaming discontinuity.** A player-defeat reset moves the anchor `164.5` units in one tick and the far field takes about `30` s to resolve (KI-030). Anything with fast travel, a second encounter elsewhere, or a load between places inherits this.
- **The runtime samples continuously; every audited claim is column-shaped.** Exact at column centres, up to `2` voxels apart away from them, and the water veto disagrees with itself in a one-column band along the waterline (KI-031). This is why the reachability audit is an upper bound.

**M7's own numbers, kept because they are what M8 moved.** Its route ran from `(-69, 49)` to an adversary `162` steps away at `(14, 191)`, `217.09` world units and `63.9` seconds of walking. M8 re-derived the adversary's placement against a landmark and the route with it: `(-135, 62)`, `69` steps, `97.17` units. The world still takes about a minute to settle — `time_to_idle_ms` was measured at `63,032`. `WASD` walks relative to the camera, the mouse looks, `J` or left mouse attacks, `K` or `Space` dodges once the adversary is awake, `F4` detaches the camera. The named route is `217.09` world units and `63.9` seconds of walking.

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

- **The M9 revisit failed its owner gate (2026-09-23).** Pre-gate PASS was headless evidence and the owner's verdict overrides it: the found weapon read as better overall. Do not reinterpret the FAIL as balance-by-preference — the owner likes bigger weapons, and no situation favouring the original was named either. M9 is not complete, and even a future PASS in the laboratory would not authorise a merge (KI-043).
- **Do not retune the found weapon again without the owner.** The owner approved exactly one retune (damage `28`, windup `36` ticks); every other found value is the frozen branch's. `FOUND_ENCOUNTER_SIGNATURE` and `REWARD_BEHAVIOR_SIGNATURE` were re-locked for it and would move again with any change.
- **Do not make the adversary weapon-aware.** The band, the lunge and the brain must never read the armament (COMBAT-005, ARM-002); `the_adversary_decides_the_same_whichever_weapon_the_player_holds` is the guard. If the found weapon breaks the band some day, the lever is the found weapon, not the adversary.
- **`weapon-choice` is a laboratory.** Its QA exchange point and its paused round start exist for the owner's test; the owner's `initiative` session and the product `traverse` session are unchanged by it.
- **No durable lock of the found weapon against initiative exists yet**, deliberately; decide after the owner's verdict whether one is warranted.
- **Combat initiative is closed, and its numbers are still not owner judgement.** OWNER PLAYTEST — PRESSURE DEMANDS RESPONSE: PASS and independent QA PASS, both 2026-09-23. The owner judged that pressure demands a response; every number in its document is headless, real-client self-QA or independent-QA evidence, and none becomes owner judgement by association. Do not retune it or give it a milestone number without the owner.
- **Do not act on the owner's remarks as if they were work items.** *"Dá pra prever"* — predictable once understood, and still fun — is recorded as an observation next to the risk that spam-read wins cleanly. The request for the great sword belongs to M9, which is frozen on its own branch and was not part of this gate.
- **Do not re-lock a historical value for this capability.** `GOLDEN_ENCOUNTER_SIGNATURE`, both weapon fingerprints and every M5/M7/M8 lock stayed byte-identical and must stay so; if one moves, the capability has leaked into the historical encounter. `COMBAT_INITIATIVE_SIGNATURE` is the lock that is allowed to move, with an OLD/NEW/WHY paragraph.
- **The spacing dodge must stay non-reactive.** Its trigger is the adversary's own state — its stagger ended, or its own lunge connected — and `dodges_during_unresolved_swing` must stay `0` for the adversary. Adding the player's action to the brain's inputs turns this into `feat/combat-pressure`, which is blocked for a measured reason.
- **Stone behind the adversary turns the capability off** (KI-041). The opt-in session stands in the open clearing on purpose; the M8 encounter at the spire does not use the capability, and moving it there is a navigation decision.
- **`combat-shoulder` frames from behind the adversary** (KI-042). Do not use it as evidence of what the player sees.
- **The golden-world initiative gates are slow in debug** — the open-clearing gate took `80` s and the spire gate `139` s under `cargo nextest` on the audited host, because each builds a world and plays dozens of sixty-second fights. They are not ignored; they are the terrain evidence.
- **No numbered milestone is active, and M9 is not complete.** The revisit was planned, approved and built; what it may become after the owner's verdict is the owner's call. Do not create another `feat/*` branch, name a milestone or treat the roadmap's capability groups as a queue.
- **The owner judged M8's discovery and nothing else.** Two destinations noticed, read as different, one chosen, walked to, adversary found. Distances, contrast, the third landmark, the gate's passability, the weather comparison and KI-032 are all self-QA evidence. Promoting any of them to owner judgement is the M7 mistake repeated.
- **The visual-simplicity and Cube-World observations are recorded, not actioned.** The owner said the structures are simple but sufficient, and that they still read close to familiar voxel-RPG architecture (KI-036). Neither is a work item on this branch: no props, no decoration, no second family, no second material system, no particles, no lights, no interiors.
- **The landmark plan is derived when a world is built, not when a chunk is generated.** It costs `316`–`409` ms in release and about `1.5` s in debug per world (KI-035). `cargo nextest` runs one process per test, so no cache helps the suite; what helps is building one world per process, which is what `WorldSelection::build` and `TerrainGenerator::with_plan` are for. Do not reintroduce a lazy or global plan to make a number look better.
- **`LANDMARK_BEHAVIOR_SIGNATURE` is the golden plan's own fingerprint** and is folded into the world fingerprint. Any change to placement, the compiler, the descriptor or the controls moves it, invalidates every cached chunk, and needs an OLD/NEW/WHY paragraph — as do `TERRAIN_BEHAVIOR_SIGNATURE`, `GOLDEN_REGION_SIGNATURE`, `GOLDEN_ROUTE_SIGNATURE` and `ADVERSARY_COLUMN`, all of which M8 moved once.
- **`TERRAIN_GENERATOR_VERSION` must not be bumped for landmark work.** `WorldSeed::stream` folds it into every stream, so bumping it reseeds the valley and regenerates a world nobody asked to change.
- **Visibility has two levels and neither is the whole answer.** The world proxy in `veldwake-procedural` answers occlusion and knows nothing about cameras; the oracle in `apps/client/src/landmark.rs` answers framing and legibility and does no raycasting. A landmark is visible when both agree, and `the_world_proxy_and_the_presentation_agree_about_what_is_visible` is what keeps the placement honest.
- **A landmark keep-out is a refusal, never a surface.** `GroundSampler` is untouched, nothing walks on a landmark, and the keep-out is the widest body's capsule — `0.86` by `2.84`, measured from the compiled rigs — applied to a destination only, exactly like the water veto.
- **The last three milestones were all proposed from the state of the repository** rather than from the order of the roadmap's list, and two of the three were renamed by the owner before they were accepted.
- **A descriptor reaches the landmark compiler only through `CompiledMonolith::new`, which validates it.** Branch QA made that constructor fallible because the informal precondition it trusted was reachable: a `band_period` of zero panicked and negative dimensions compiled an empty landmark. Do not add a second path into the compiler that skips the check.
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
