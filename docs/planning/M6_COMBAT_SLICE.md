# M6 — Combat Slice

Status: **in progress on `feat/m6-combat-slice`**
Base: `main` at merge commit `5bebc4fa5195b427216a296fff810a294b7bbd7b`

M5 proved a person can stand in the world. M6 has to prove that a person can *fight* in it — that a player approaches, reads an intent, decides, commits, connects, is hit, reacts, and does it again. The exit criterion is the same kind M4 and M5 used: a real run of the real client on the audited host, played by a person, judged against a written contract, backed by headless tests and measurements.

This document is written as the milestone lands, phase by phase. Anything not yet measured is not claimed.

## Scope, copied from the roadmap

> One weapon and one enemy with movement, attack/defense or dodge, telegraphs, hit reaction, camera response, procedural impact audio, VFX, animation, and encounter/readability playtest evidence.

Nothing else. This is a vertical slice of combat, not a combat system.

## Phases

| phase | contents | status |
|---|---|---|
| M6A | `veldwake-combat`: tick clock, weapon compiler, attack specs, combatants, actions, hit sweep, movement, adversary, encounter, fixtures, `combat-probe` | in progress |
| M6B | client: input, encounter modes, accumulator, two actors, weapons, follow camera, armed gate, health readout, debug volumes, first real-game run | not started |
| M6C | action motion, reaction, knockback, hitstop, camera shake, named moments, capture cycle | not started |
| M6D | VFX and procedural impact audio, with measurements | not started |
| M6E | playtest evidence, regressions, performance, documentation close-out | not started |

## The two corrections the design carried

The approved design had two defects that were corrected before any code was written, and both are worth keeping stated because the wrong version was plausible.

**An authoritative step must not accept an arbitrary `dt`.** `Encounter::step(input, ground)` advances exactly one tick. Every duration in the domain — attack phases, dodge, stagger, hitstop, adversary timers, defeat hold — is a tick count, not a float of seconds. `NaN`, negative, and infinite time are unrepresentable rather than rejected, and a phase boundary is an integer comparison rather than a float one. Seconds exist only in authored configuration, which is compiled to ticks by a validating constructor before it can reach the runtime.

**A weapon is an object; an attack is a decision.** The first design put `AttackProfile` inside `WeaponDescriptor` while giving the player and the adversary different windups — which would have meant two different weapons wearing one name. `WeaponDescriptor` now carries only what makes the weapon a physical object (dimensions, palette, derived blade segment, identity). Timing, damage, step-in and reach tuning live in `AttackSpec`, of which M6 has exactly two — `player` and `adversary` — both executing the *same compiled weapon*. That keeps "one weapon" literally true and lets the adversary telegraph for two and a half times as long without falsifying weapon identity.

## M6A — the combat domain, headless

### A new crate: `veldwake-combat`

The crate test in [`../engineering/ARCHITECTURE.md`](../engineering/ARCHITECTURE.md) was applied before the directory existed:

- **Ownership.** Combat rules are a distinct domain with their own identifier range, their own contract version, and their own fixtures — the same argument that qualified `veldwake-character`.
- **Dependency direction.** The rules need `character` (bones, poses, capsule, gait) and therefore sit above it. Putting them *inside* `character` would make the animation crate own gameplay rules, and `ARCHITECTURE.md` lists `gameplay` and `animation` as separate domains. Putting them in the client would bury testable authority in platform code, against [ADR-0002](../adr/0002-presentation-independent-authority.md).
- **Build isolation.** `character-probe` and the character suite must not compile hit sweeps, adversary logic and encounter scripts they never call.
- **Testability.** The whole domain runs with no GPU, no window, no device and no world generator.

```text
voxel <- character <- combat <- client
```

`veldwake-combat` depends on `veldwake-character`, `veldwake-voxel`, `glam` and `std`, and on nothing else. It contains no `wgpu`, `winit`, `cpal`, `veldwake-procedural` or `veldwake-streaming` types. What it needs from the world is the height under a foot, which is the `GroundSampler` trait `veldwake-character` already declares and the client already implements over `TerrainField`.

### The tick clock

`COMBAT_TICK_HZ = 120`. `Encounter::step` is one tick; there is no `dt` parameter anywhere in the crate.

The client converts wall time to a tick count with integer arithmetic only:

```text
scaled += elapsed.as_nanos() * COMBAT_TICK_HZ      (u128)
available = scaled / 1_000_000_000
scaled    = scaled % 1_000_000_000                 (sub-tick remainder preserved)
run       = min(available, MAX_TICKS_PER_FRAME)
dropped   = available - run                        (counted, reported, never merged into a big step)
```

Because the remainder carries, the total number of ticks after a given elapsed time is `floor(total_nanos * 120 / 1e9)` **whatever partition of frames delivered it**. That is the claim this representation supports, and it is the only claim made: for the same representable elapsed time, with no catch-up cap hit, the same number of ticks in the same order is produced. Human input timing relative to ticks is *not* claimed to be partition independent — which is exactly why the scripted encounter keys its input to tick indices, and is therefore bit-identical at 30, 60 and 144 Hz.

`MAX_TICKS_PER_FRAME = 4` (33.3 ms of catch-up). Beyond it the excess is discarded and counted; the simulation slows rather than teleporting.

### The canonical tick order

The order is part of the determinism contract, so it is stated and tested rather than left to emerge from loop order:

```text
1. hitstop      decrement; a frozen combatant advances no action timeline and no gait phase
2. intent       player from the latched input, adversary from its brain (brain decides only when the action is Free)
3. action       transitions, then timeline advance in ticks
4. movement     fixed order [Player, Adversary]: proposed planar move, step rule, ground, arena, realized speed
5. pose         character::pose_with(overlay); blade world segment recorded, previous kept
6. hit          fixed order [Player, Adversary]: sweep previous->current blade against the foe's hurt capsule
7. consequence  damage, stagger, knockback, hitstop, events
8. separation   capsule push-out through the same movement rules, then re-ground
9. outcome      defeat hold, reset
```

Two policies follow from that order and are documented rather than accidental:

- **Simultaneous hits resolve in the fixed order Player then Adversary, and a combatant defeated earlier in the same tick does not land its own swing.** Death cancels the swing. This deliberately gives the player the tie.
- **Hitstop set by a hit takes effect from the next tick**, so it cannot reorder anything inside the tick that produced it.

### Bounded events

`StepEvents` is a fixed array plus a length, not a `Vec`. A reserved-capacity `Vec` proves nothing about a bug that exceeds the capacity; a fixed array makes overflow representable, counted and asserted to be zero on every fixture.

### The weapon

One weapon, compiled from a descriptor by code, with no modelling tool and no new mesher.

- Voxels are written into one reused `32³` scratch grid and meshed by `veldwake-voxel`'s existing `mesh_exposed_faces`, at the character's `1/12` world unit so the two domains share a scale.
- `WeaponMaterial` owns the declared range `192..=223` — named for a weapon rather than for a general "equipment domain", because M6 compiles one weapon. Disjointness against air, the M2/M3 diagnostic identifiers, terrain and characters is proved in the crate and again in the client, where all four tables are visible at once.
- The blade's collision segment is **derived from the compiled voxels** by scanning for blade materials, never declared beside them, so it cannot drift from the geometry the viewer sees.
- The grip is one rigid `Transform` in `HandR` bone space. The weapon never moves relative to the hand; the arm moves. "Weapon detached from hand" is therefore impossible by construction rather than by vigilance.

### Hit detection

The blade segment is swept, not tested for overlap once a frame.

For each Active tick, both endpoints of the blade segment are kept from the previous tick and interpolated to the current one. The substep count comes from the *larger* of the two endpoint displacements, which is what makes a rotation about the hand safe:

```text
advance = max(|base_end - base_start|, |tip_end - tip_start|)
n       = clamp(ceil(advance / CHARACTER_VOXEL_SIZE), 1, MAX_SWEEP_SUBSTEPS)
```

Each substep is a segment-segment closest-distance test against the foe's hurt capsule axis, in closed form. `MAX_SWEEP_SUBSTEPS` is bounded and its high-water mark is reported.

The hurt volume is the compiled body capsule placed at the combatant's stand point. Its consequence is accepted and turned into an asserted property rather than a hidden limitation: for every named moment, the head, torso and pelvis cores are inside the capsule, while the arms, hands and the weapon may leave it. A swing that only grazes an outstretched arm does not register, and that is stated in [`../KNOWN_ISSUES.md`](../KNOWN_ISSUES.md) instead of being discovered later.

### Movement and world contact

No physics engine, and Rapier specifically remains rejected. The contact problem is "which block is under this foot", which `GroundSampler` answers exactly and a solver answers approximately — and CHAR-002 makes the exact answer an invariant. Knockback here is a bounded kinematic displacement, not a response to force, so the review trigger [ADR-0004](../adr/0004-rigid-voxel-character-and-analytical-locomotion.md) set for a physics engine has not fired.

The rules are: a proposed planar move is accepted only if the destination is inside the arena, has ground, and its surface is within the step-up and drop bounds; a rejected move is retried one axis at a time so a body slides instead of sticking; `surface == None` rejects. Separation between the two combatants reuses that same acceptance rule, so a push-out can never place a body outside the arena, off the ground, or over a forbidden step, and a blocked actor's share of the separation is deterministically given to the other.

### The adversary

A small explicit state machine — `Idle`, `Approach`, `Reposition`, `Recover` — that produces *intent* and nothing else. It does not carry a second attack timeline: while the authoritative action is not `Free` the brain holds. Six states and a behaviour-tree library for eight transitions is the overengineering R-002 names, and a utility system needs utility functions to choose between three options.

Variation comes from a named deterministic stream (`CombatSeed::stream(label)` plus an explicit decision index), never from ambient or sequential randomness.

## M6B — the playable loop in the client

`VELDWAKE_ENCOUNTER` selects the mode, and `off` is the default and a regression contract: with it unset the client behaves exactly as it did at `5bebc4f` — free-fly camera, `VELDWAKE_CHARACTER` untouched, no combat simulation, no second actor on the GPU, no weapon, no particles, no audio device, and no extra draw or upload.

## M6C — action motion

The action layer lives in `veldwake-character` because that crate owns "what angle is every joint at". Combat says *which action and how far through*; the curves, the blending and the joint clamps stay in one place. Locomotion remains distance driven; the action layer is tick driven, and that second category is what [ADR-0006](../adr/0006-action-pose-layer-beside-analytical-locomotion.md) records.

## M6D — VFX and procedural audio

Deliberately last. If the combat does not feel right with the presentation extras off, particles and sound will not fix it; they are added after the core is judged, and the judgement is repeated with them on.

## M6E — evidence

The exit gate is the real game on the audited host, played by a person, with fresh captures opened and inspected.

## Scope, and what this milestone deliberately does not build

No weapon system, second weapon, weapon switching or sheathing; no second enemy; no loot, inventory, equipment, stats, skill tree, crafting or quests; no multiplayer; no AI framework, combat framework or ability system; no animation graph; no physics engine; no ECS; no navmesh or pathfinding; no boss; no procedural creature ecosystem. Specifically also: no lock-on, no stamina or posture, no invulnerability frames, no combos, no attack cancelling, no parry or block, no jumping or falling, no animated strafe or backpedal, no dodge roll, no ragdoll, no vegetation collision, no camera occlusion solving, no line-of-sight raycast, no HUD or UI framework, no pause or death screen, no encounter persistence, no music, no reverb, and no automated visual regression.
