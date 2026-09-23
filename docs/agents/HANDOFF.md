# Current handoff

Last updated: 2026-09-22

## Current position

**M9 PRODUCT GATE: FAIL — mechanical sidegrade exists, but the current encounter does not make weapon choice meaningfully affect play.** The implementation stays on `feat/m9-meaningful-reward` and is **not reverted, not merged, has no pull request, and is not ready for branch QA as a completed milestone.** `main` is unchanged at `ee35f62f97afbe3d001a27a576e9bae21e77c4d2`; the branch is five commits past `docs/post-m8-handoff` at `658ebfbd618b1d7387eee7890d11af53b5f8e045` and six ahead of `main`, counted with `git rev-list` and including the documentation commit that closed the redesign investigation.

**Redesign investigation: closed, and M9 needs a broader combat slice.** The owner accepted this outcome on 2026-09-22 and M9 is **not being expanded to build it**. Bounded changes inside the current encounter vocabulary were tried in scratch harnesses outside the repository and exhausted as a next step: across every one of them the owner's approach-and-attack strategy remained a reliable six-of-six victory in every ordinary golden approach with both weapons. The decisive fact is geometric — the player closes the distance at `3.4` world units per second while attacking, the adversary repositions at `1.8`, and its movement intents are clamped to the same `3.4` ceiling, so no ordinary movement or state change can deny a pursuing attacker. The weapons' mechanical differences remain valid and measured. The investigation, with owner, repository and scratch evidence kept apart, is in [`planning/M9_MEANINGFUL_REWARD.md`](../planning/M9_MEANINGFUL_REWARD.md#redesign-investigation--closed) and KI-039.

**The next step is a separate combat milestone proposal, not M9 implementation.** It starts from `main`, not from this branch, and its product question is independent of the reward: *can the adversary force the player to respond to pressure rather than win by holding forward and repeatedly attacking?* The missing category is a broader adversary capability for defending against and managing pressure; a burst evasive action — possibly the existing `Dodge` used by the adversary — is one plausible first experiment and a hypothesis only, not a selection. No milestone number is assigned. M9 would later consume that capability and rerun its own owner gate.

**OWNER PLAYTEST — WEAPON CHOICE MATTERS: FAIL**, 2026-09-22. The owner played both weapons. Unprompted, the natural strategy was to approach the adversary, stand facing it, attack repeatedly and win — and then to do essentially the same thing with the other weapon. Asked for a second session focused deliberately on the differences, the owner **did perceive the difference in distance and reach** and **did perceive the difference in the attack's timing and motion**, and still reported that combat stays simple enough that neither changes the strategy significantly: *"sempre só acaba acertando ele de qualquer jeito"*.

**Read the verdict precisely, because the obvious paraphrase is false.** The weapons are mechanically and perceptually distinguishable and the owner distinguished them. The gate fails because **both weapons support effectively the same natural combat strategy** — the reward changes measurable weapon properties without yet changing the player's meaningful combat decisions. No document may record that the weapons feel identical, that the found weapon failed to communicate its longer reach, that it is universally superior, or that the owner could not perceive the timing difference. All four would be false, and each one would send the next session to retune weapon numbers instead of looking at the fight.

**The finding is about the encounter, not the weapons.** The current M6 encounter may be too permissive to expose a sidegrade at all: against stand-in-front-and-swing, reach, commitment, whiff exposure and dodge availability exist technically and can be ignored. That is **KI-039**, recorded with no solution prescribed and none chosen. It is not evidence that the combat system is bad — M6's own owner gates on readability, telegraph, impact and camera were passed by the same person and are not reopened by this.

**Every technical result stands and is unchanged.** The found weapon reaches further, commits longer, exposes a longer whiff and opens a five-fold wider standoff band; the exchange works; the armament works; ARM-001 holds through a real defeat; the M6 locks are byte-identical; the world and chunk state is untouched. A product gate and a technical gate are different gates and both results are real.

To play what exists:

```text
VELDWAKE_ENCOUNTER=traverse VELDWAKE_PROFILE=m4-golden cargo run --release -p veldwake-client
```

**The owner had read the M9 architecture report before playing**, so M9 may never claim owner-observed *discovery* of the object or its location. M8 owns discovery; any visibility or composition claim in M9 is technical and self-QA evidence.

**What M9 is.** The first proof of Mastery in this project. A longblade stands in the gate's opening; `E` exchanges it for the weapon in the player's hand; the site keeps whichever one the player is not carrying; returning reverses the choice. The two are a sidegrade: the found weapon connects out to `3.4477` world units against the original's `2.8835` and locks for `103` ticks against `75`, and against the adversary's `2.40` u/s approach the extra reach buys `28` ticks and the extra commitment costs exactly `28`. It fells the adversary in three swings instead of four, and each miss exposes `37%` longer.

**What M9 is not**, and none of it is an oversight: persistence, an inventory, slots, loot, rarity, drops, progression, stats, a second enemy, a second reward site, a third weapon, a third actor, an entity model, object collision, an interaction framework, or a two-handed weapon. The found weapon hangs off `HandR` exactly as M6's does and the free hand does nothing; **the documentation must never call it two-handed.**

**Read [`../planning/M9_MEANINGFUL_REWARD.md`](../planning/M9_MEANINGFUL_REWARD.md) before touching anything M9 built.** Its durable decision is [ADR-0010](../adr/0010-session-acquired-state-in-the-authoritative-encounter.md) — session-acquired state in the authoritative encounter — and its two invariants are **ARM-001** (an encounter reset restores the bodies and never the armament) and **ARM-002** (one weapon selector, and the adversary is outside the exchange pair), both in [`../engineering/INVARIANTS.md`](../engineering/INVARIANTS.md).

**Facts a next session must not rediscover.**

- **`crates/procedural` is not modified by M9. Not one line.** The exchange site is derived from the gate M8 already built: `LandmarkInstance::crown_column` is the lintel over the opening, so it is the opening's centre, and the anchor is one column along the gate's span axis from it. No search, no lattice, no candidate set, and therefore nothing added to the eager landmark plan KI-035 measures.
- **No chunk byte changes, so no world lock moved.** The world fingerprint, `TERRAIN_BEHAVIOR_SIGNATURE`, `GOLDEN_WORLD_BEHAVIOR_SIGNATURE`, `LANDMARK_BEHAVIOR_SIGNATURE`, `GOLDEN_REGION_SIGNATURE`, `TRAVERSAL_RULE_VERSION`, `GOLDEN_ROUTE_SIGNATURE`, `ADVERSARY_COLUMN`, every M5 lock and all three M6 weapon and encounter locks are byte-identical. `resolving_a_reward_writes_no_voxel_into_the_world` is the tripwire that would catch a future change making placement reach into generation.
- **`COMBAT_STYLE_VERSION` stays at `1`, on purpose.** It is hashed into every `WeaponIdentity`, so bumping it for a second accepted weapon would move the historical M6 weapon's identity while its descriptor, geometry and behaviour are untouched. The M9 profile is additive under the same grammar and carries its own `FOUND_WEAPON_PROFILE_VERSION`. Do not bump the shared version to tidy this up.
- **`GOLDEN_ENCOUNTER_SIGNATURE` is `0x6415_7522_d253_5658` and must stay there.** If it moves, that is a defect to explain, not a lock to update.
- **The aim-assist range is derived and floored.** `max(AIM_ASSIST_RANGE, connects_out_to(other body))`. Both M6 bodies connect out to less than `2.90` — `2.8835` and `2.7439` — so the `max()` returns the literal for both and M6 is exact by arithmetic. The found weapon gets `3.4477`. Leaving the constant fixed was measured to collapse the found weapon's usable facing window from `69` degrees to `26`.
- **`reach::attack_envelope` is production code and `fixture.rs` no longer owns it.** The encounter's aim rule, the fixtures and `combat-probe` all call it. Do not write a second copy of the formula.
- **Every weapon-dependent path goes through `Encounter::weapon_of`** — the world matrix, the blade segment, the sweep radius, the attack spec and the aim range. There is no remaining bare `self.weapon` where a player's choice should matter, and the adversary always resolves to the original (ARM-002).
- **Input priority is attack, then dodge, then interact**, and a press a higher verb consumed is consumed rather than held. A test runs two hundred ticks after an attack-plus-interact press and asserts nothing fires.
- **An interact is not suppressed while the adversary sleeps**, unlike a dodge. Visiting the gate before any fight is the point of the milestone, and an interact-only first input arms the session.
- **Three weapon instances exist in a running session, not two.** The exchange pair is the player's and the site's; the adversary carries an independent original outside it. The client reports `weapons=3`. Do not write "there are exactly two weapons in the world".
- **The planted weapon is non-colliding.** A body walks through it, which is why `TRAVERSAL_RULE_VERSION` did not move and the gate is exactly as walkable as M8 left it.
- **The anchor is one column off the opening's centre, and that was a correction with evidence.** The centre is also the line a body walks and the line the camera looks down; the captures at three world units showed the weapon occluding the torso and both legs with only its shadow reading as a sword. `REWARD_BEHAVIOR_SIGNATURE` was re-locked once for it, `0x0815_132f_1a6b_7572` to `0x0f08_fbf7_08e3_206d`, and the paragraph naming why is in `reward.rs`.

**Two findings recorded rather than fixed, and both matter to whatever comes next.**

- **KI-038: the adversary has no navigation, and M8 gave it a wall.** Approached from the east it steers straight into the spire's keep-out and stops — `brain="approach"`, position frozen, separation stuck at `8.72` units — because M9 is the first thing that gives a player a reason to arrive from a direction M7's derived route never used. The same session worked from the south. Nothing in M9 caused it and nothing in M9 addresses it.
- **KI-037: the two weapons sound identical.** `intensity = damage / victim_max_health * 4.0` clamped to one saturates at M6's `24` damage, so the found weapon's `32` produces exactly the same voice. Measured, recorded, and deliberately not fixed — the owner ruled no audio work belongs in M9.

**What the owner gate settled**: the two weapons *do* feel different to a person, in reach and in timing. **What remains unverified**, and no document may claim otherwise: whether the found weapon reads as belonging in this world or as generic loot, whether its scale is imposing or absurd, and a player victory in the real client — the closed loop took the adversary to `64` twice and never to `0`, so victory-preserves-armament is headless evidence only.

**M8 — Discoverable Landmarks is complete and merged. M1 through M8 are all in `main`.** It landed through [PR #12](https://github.com/Jovinull/veldwake/pull/12) at merge commit `ee35f62f97afbe3d001a27a576e9bae21e77c4d2`, whose parents are `0c81c069bb95a8caf3b6a5252b89b023c6334ae1` and `d5e200bbd18d7c5aee167151509b89be260b85e2`, after the owner's discovery playtest, independent branch QA, a green pull-request CI run ([run 35732875867](https://github.com/Jovinull/veldwake/actions/runs/35732875867)) and a green post-merge CI run on the merge commit ([run 35735467195](https://github.com/Jovinull/veldwake/actions/runs/35735467195)). The remote branch `feat/m8-discoverable-landmarks` is preserved at `d5e200bbd18d7c5aee167151509b89be260b85e2`, the head where the 755 tests and every gate were run. The workspace has **755 tests**, three of them `#[ignore]`d; all **758** pass when the ignored ones are run explicitly.

**There is no next milestone.** That is deliberate and it is the first thing to understand before doing anything else.

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
19. [`../audiovisual/COMBAT_STYLE.md`](../audiovisual/COMBAT_STYLE.md) — the versioned, checkable constraints M6 was held to: the weapon, the five action poses, the two effects, the readout, and what the camera may do
20. [`../design/COMBAT.md`](../design/COMBAT.md) — the design intent combat aims at, most of which M6 deliberately does not build yet
21. [`../planning/M9_MEANINGFUL_REWARD.md`](../planning/M9_MEANINGFUL_REWARD.md) — the current milestone: the weapon exchange, the measured sidegrade, the one visual correction it made, and what stopped for the owner
22. [`../planning/M8_DISCOVERABLE_LANDMARKS.md`](../planning/M8_DISCOVERABLE_LANDMARKS.md) — the milestone before it: the composition, the owner's session, the branch QA section, and what nobody could verify
22. [`../planning/M7_TRAVERSABLE_REGION.md`](../planning/M7_TRAVERSABLE_REGION.md) — the milestone before it: the owner decisions it answers to, the two architectural corrections it carries, the whole-region reachability result, the named route and everything measured
23. [`../planning/M6_COMBAT_SLICE.md`](../planning/M6_COMBAT_SLICE.md) — the milestone before it: its evidence, its measurements, the six branch-QA findings, the owner gates, and its limitations
24. [`../planning/M5_PROCEDURAL_CHARACTER.md`](../planning/M5_PROCEDURAL_CHARACTER.md) — the milestone before it, and the character every combat pose is built on
25. [`../adr/0010-session-acquired-state-in-the-authoritative-encounter.md`](../adr/0010-session-acquired-state-in-the-authoritative-encounter.md) — why the armament is authority, why its lifetime is the session, and why that is not yet persistence
26. [`../adr/0009-two-level-landmark-visibility-and-eager-world-plan.md`](../adr/0009-two-level-landmark-visibility-and-eager-world-plan.md) — why landmark visibility is answered at two levels and why the world plan is derived eagerly and typed
26. [`../adr/0008-traversal-legality-separate-from-ground-contact.md`](../adr/0008-traversal-legality-separate-from-ground-contact.md) — why a traversal veto sits beside `GroundSampler` instead of changing it, and why the veto never reports a height
27. [`../adr/0005-fixed-step-headless-combat-domain.md`](../adr/0005-fixed-step-headless-combat-domain.md) — why combat is integer-stepped and headless, and why two combatants are the whole entity model
28. [`../adr/0006-action-pose-layer-beside-analytical-locomotion.md`](../adr/0006-action-pose-layer-beside-analytical-locomotion.md) — why a tick-driven action layer sits beside distance-driven locomotion, and why ADR-0004 was not superseded
29. [`../adr/0007-procedural-impact-audio-boundary.md`](../adr/0007-procedural-impact-audio-boundary.md) — why the synth has no I/O, and where the device boundary is
30. [`../adr/0004-rigid-voxel-character-and-analytical-locomotion.md`](../adr/0004-rigid-voxel-character-and-analytical-locomotion.md) — why body parts are rigid and locomotion is analytical, and what that costs
31. [`../KNOWN_ISSUES.md`](../KNOWN_ISSUES.md) — every accepted limitation and every closed one, with the reason each was closed
32. [`../LEARNINGS.md`](../LEARNINGS.md) — reusable discoveries below ADR scope
33. [`EVIDENCE_HARNESS.md`](EVIDENCE_HARNESS.md) — how a capture is produced, what makes one invalid, and how it picks the right window
34. [`WORKFLOW.md`](WORKFLOW.md) — how an agent is expected to work in this repository

Then, as needed: [`../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md`](../planning/M4_BEAUTIFUL_TERRAIN_SLICE.md), the rest of the [ADRs](../adr/README.md), and [`../environment/SETUP.md`](../environment/SETUP.md) for gate commands and environment variables.

Then the code. Read it in this order, because it is the surface anything after M7 works against:

- `crates/combat/src/movement.rs` — **the whole of movement legality**, in one place. `check_move` returns `MoveBlockReason`, `accepts` wraps it, `try_move` slides per axis, `separate` pushes two bodies apart through the same rules, and `TraversalLegality` is the veto a world hands in. Read this before anything that moves a body.
- `apps/client/src/traversal.rs` — the region reasoned about as columns: the water veto, the cached `SurfaceGrid`, the whole-region reachability audit that calls `check_move`, the derived and locked named route, the derived adversary placement, and the tests that judge all of it against the generated voxels rather than against each other.
- `crates/combat` — the fixed-step headless domain. One call to `Encounter::step` is one tick; every duration is a tick count; the weapon, the two attack specs, the swept hit query, the torso-column hurt volume, the kinematic movement rules, the adversary's state machine and the bounded event type all live here. It has no GPU, window, audio or filesystem types, and it depends on `crates/character` and never the reverse.
- `crates/character` — the descriptor, compiler, skeleton, distance-driven locomotion, ground contact, IK and collision representation, plus the tick-driven action pose layer M6 added beside it. `pose()` is M5's path and is unchanged; `pose_with()` takes an action overlay.
- `apps/client/src/encounter.rs` — where the domain meets the frame: the integer tick accumulator, the encounter modes including the frozen named moments, and the bounded per-frame event buffer.
- `apps/client/src/arena.rs` — where in the world a fight happens, and how a named camera pose is placed against the two bodies rather than against the map.
- `apps/client/src/input.rs` and `apps/client/src/camera.rs` — **both camera models live here.** With the encounter off the free-fly camera and raw keyboard state are exactly what they were before M6. With an encounter armed, input is latched on the key-down edge and interpreted relative to a third-person follow camera, and `HitShake` moves that camera on a confirmed hit and on nothing else.
- `apps/client/src/vfx.rs`, `readout.rs`, `synth.rs`, `audio.rs` — presentation that is **driven by `CombatEvent` and never by a guess**: chips and a telegraph accent from one fixed pool, the eight-pip health readout, the pure synth with no I/O, and the one cpal device behind a lock-free queue.
- `apps/client/src/renderer.rs` and the WGSL beside it — the world, shadow, character and effect passes, and the one shared lighting function terrain and bodies both call. Actors and weapons upload once; a frame writes transforms.
- `crates/procedural`, `crates/streaming` and `apps/client/src/world.rs` — how terrain is generated, made resident, and reaches the client. Combat touches this only through the `GroundSampler` trait.

The shapes worth holding while reading: the domain is authoritative and headless, the client advances it in whole ticks, and every visible or audible consequence of a fight is derived from an event the rules published.

## History — the post-M8 continuation point, spent

M9 was proposed from this section and accepted by the owner with corrections, so
its *instructions* are spent. Its facts are not, and the list below is still the
ground anything after M9 stands on — in particular that the roadmap's capability
groups are not a queue, that the last four milestones were each proposed from the
state of the repository, and that M9 answered exactly one of the gaps it names:
there is now a reason to go somewhere beyond arriving. The rest are untouched.

## Continue here — propose a combat milestone, separate from M9

**Do not start a milestone. Propose one.** The evidence now points at one: M9's redesign investigation closed on the conclusion that the adversary lacks a capability for managing pressure, so the natural next proposal is a combat milestone built from `main` whose own product question is whether the adversary can force the player to respond to pressure rather than win by holding forward and attacking. It must not be built by expanding `feat/m9-meaningful-reward`, it has no number yet, and no capability — enemy dodge included — has been chosen for it. Read the investigation in [`../planning/M9_MEANINGFUL_REWARD.md`](../planning/M9_MEANINGFUL_REWARD.md#redesign-investigation--closed) first; its negative results are what keep the proposal from re-tuning numbers.

[`../planning/ROADMAP.md`](../planning/ROADMAP.md) lists *later capability groups* — persistence and world editing, aggregate and local world simulation, settlements, history and economy, richer procedural assets, audio and music, multiplayer transport, WASM modding. **That list is not a queue and nothing in it has been chosen.** No name is reserved, no branch exists, and "the next one on the list" is not a decision. M7 and M8 were both proposed from the state of the repository and both were renamed by the owner before they were accepted.

The next session's job is to read what now exists, weigh it against those groups *and against the evidence M8 produced*, and propose one milestone — its scope, its exit criteria, and what it deliberately will not build — for the owner to accept before any code is written.

**M8 produced new facts that should change how that proposal is reasoned about.** They are the point of writing them down here:

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

### Where the code is, and in what order to read it

- `crates/procedural/src/landmark/` — `material.rs`, `descriptor.rs`, `compile.rs`, `visibility.rs`, `plan.rs`. Read `plan.rs` last: it is the composition, and the rest is what it composes.
- `crates/procedural/src/generator.rs` — where the plan is derived, and where landmark voxels are written into air after vegetation.
- `crates/procedural/src/vegetation.rs` — `WorldVegetation`, the one composed answer about plants.
- `apps/client/src/traversal.rs` — the keep-out, the surface grid, the audit, the route and the adversary's placement.
- `apps/client/src/landmark.rs` — the presentation oracle and the capture poses.
- `crates/procedural/src/bin/terrain-probe.rs` — `landmarks` prints the plan, re-derives it and refuses to print if the two disagree.

### What is proven, and by whom

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

- **M9's product gate failed and its redesign investigation is closed.** No pull request, no merge, no further M9 scope, and no retune of the weapons, the adversary, the aim assist or the tuning. Damage, health, aim assist, stagger, telegraph, strike range, hold band, brain `Recover`, adversary attack recovery, commit point (K), recovery punishment (P) and threat-aware approach (T) were all tried and rejected as the next step. The commit point, the recovery punishment and the rise-and-hold windup exist only as scratch findings: none is approved and none is in the repository. Making the found weapon artificially weaker to manufacture a contrast would answer a different question than the one that failed.
- **Do not paraphrase the failure into "the weapons feel the same".** The owner perceived the reach difference and the timing difference. The failure is that the encounter does not ask the player to act on either, which is a different problem with a different solution space (KI-039).
- **The owner already knows what the reward is and where it is**, because the owner read the architecture report. Any claim of owner-observed discovery, visibility or composition in M9 would be false. M8 owns discovery; M9 asked only whether the choice matters.
- **Do not bump `COMBAT_STYLE_VERSION`.** It is hashed into every weapon identity, and bumping it would move the M6 weapon whose descriptor, geometry and behaviour are untouched.
- **Do not re-lock `GOLDEN_ENCOUNTER_SIGNATURE`, any world signature, `TRAVERSAL_RULE_VERSION`, `GOLDEN_ROUTE_SIGNATURE` or `ADVERSARY_COLUMN`.** All of them are byte-identical on this branch and a movement is a defect to explain.
- **`crates/procedural` must stay untouched by M9 work.** If a change appears to require generating different chunk bytes, stop and report rather than changing the cache-identity decision.
- **The M9 weapon is a longblade held in one hand.** It is not two-handed, there is no second-hand contact and there is no two-handed pose. Writing otherwise would describe a feature that does not exist.
- **The exchange is session state and nothing persists.** A restart returns to the authored pair. Do not add a save to make the reward feel better; that is the next milestone's decision to take deliberately.
- **A blind driver cannot aim.** Four M9 runs produced zero player hits; only a closed observe-decide-act loop landed any. Any future claim about hits, victories or feel from a scripted run is measuring the harness.
- **The adversary can be walled off by a landmark (KI-038).** Approaching the spire from the east leaves it stuck. Do not read that as an M9 regression, and do not fix it inside M9.
- **There is no milestone after M9, and that is the state, not an omission.** Propose before coding. Do not name a milestone, do not create a `feat/*` branch, and do not treat the roadmap's capability groups as a queue.
- **The owner judged discovery and nothing else.** Two destinations noticed, read as different, one chosen, walked to, adversary found. Distances, contrast, the third landmark, the gate's passability, the weather comparison and KI-032 are all self-QA evidence. Promoting any of them to owner judgement is the M7 mistake repeated.
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
