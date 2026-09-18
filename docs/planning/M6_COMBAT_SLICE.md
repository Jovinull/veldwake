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
| M6A | `veldwake-combat`: tick clock, weapon compiler, attack specs, combatants, actions, hit sweep, movement, adversary, encounter, fixtures, `combat-probe` | complete |
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

### Headless evidence, M6A

The workspace has **586 tests** (38 voxel, 69 procedural, 97 streaming, 126 character, 168 combat, 88 client). Every M5 signature is unchanged, which is the point: `pose()` is untouched, so `GOLDEN_POSE_SIGNATURE`, the geometry, skeleton and collision fingerprints and all three behaviour signatures are byte for byte what M5 locked. The character crate gained one new locked constant, `GOLDEN_ACTION_POSE_SIGNATURE`, for the eleven named action poses.

**The weapon**, from `combat-probe weapon`:

| measure | value |
|---|---|
| volume | `6 x 23 x 4` character voxels, origin `[-3, -19, -2]`, hand row `19` |
| bands (grid `y`) | blade `[0, 14)`, guard `[14, 16)`, grip `[16, 21)`, pommel `[21, 23)` |
| solid voxels / quads | 212 / 352 |
| CPU mesh payload | 47,872 bytes |
| transient scratch | 65,536 bytes, one reused `32³` grid |
| blade segment, derived | base `(3, 14, 2)`, tip `(3, 0, 2)`, radius `1` voxel |
| blade length / radius | `1.1667` / `0.0833` world units |
| reach from the wrist | `1.5833` world units |
| identity / geometry | `0x084bf386500bb0e4` / `0x8a6b18edd4a2b879` |

**The two combatants**, from `combat-probe bodies`. The adversary got its own descriptor because a test refused the first design: reusing M5's `sturdy` fixture as the enemy gave a body that is *shorter* than the player and shares its absolute hip width of eight voxels and its limb thickness of three.

| measure | player | adversary |
|---|---|---|
| height | 28 voxels (`2.3333` units) | 33 voxels (`2.7500` units) |
| shoulder span / hip / waist | 12 / 8 / 6 | 16 / 10 / 8 |
| limb thickness | 3 | 4 |
| arm / leg / foot | 12 / 14 / 5 | 14 / 15 / 8 |
| hurt capsule radius | `0.6274` | `0.8563` |

The palette carries a second finding. **M5's three garment schemes share one luminance ladder on purpose**, so their primary tunics sit within a ten-thousandth of each other and no choice of scheme can separate two characters by value on the garment. The two are separated by size, by hue on the tunic — rust linen is led by red, slate wool by blue — and by value on the skin, where the declared tones do differ.

**The timelines**, from `combat-probe spec`, at `120` Hz:

| | windup | active | recovery | total | stagger | hitstop | damage | step-in | knockback |
|---|---|---|---|---|---|---|---|---|---|
| player | 22 (`0.183` s) | 12 (`0.100` s) | 41 (`0.342` s) | 75 (`0.625` s) | 36 | 8 | 24 | `0.35` | `0.35` |
| adversary | 54 (`0.450` s) | 14 (`0.117` s) | 72 (`0.600` s) | 140 (`1.167` s) | 36 | 8 | 18 | `0.45` | `0.30` |

Active windows are the half-open tick ranges `[22, 34)` and `[54, 68)`. The dodge is 36 ticks over `2.2` world units, so `7.33` u/s against a walk of `3.4`; its cooldown is 30 further ticks. Both bodies have 96 health, so the player falls in six of the adversary's hits and the adversary in four of the player's.

**The swing envelope**, from `combat-probe reach`, which is what the ranges were chosen against rather than from arithmetic:

| | max tip reach | active reach | active height | connects out to |
|---|---|---|---|---|
| player | `2.0561` | `0.7660` to `2.0561` | `0.9356` to `2.9037` | `2.9124` |
| adversary | `2.1165` | `0.7352` to `2.1165` | `1.1775` to `3.1654` | `2.7439` |

The adversary commits at `2.2` centre to centre. **The first draft of that number was `2.6`, which is outside its own reach**: the swing only landed because of the step-in, so the telegraph was a bluff. It is now comfortably inside.

**The dodge window**, from `combat-probe dodge`. The adversary's swing is 140 ticks; a dodge pressed on each of them, in a fresh encounter each time, either escapes or does not:

| dodge direction | escapes when pressed | last escaping press | of a telegraph of |
|---|---|---|---|
| away | ticks 1 to 51 | 51 | 54 |
| lateral | ticks 1 to 45 | 45 | 54 |

So a player has about four tenths of a second to read the swing, and the window closes three ticks — twenty-five milliseconds — before the blade goes live. Dodging away buys distance; dodging sideways leaves the plane the blade sweeps in, and the two have different margins for different reasons. **Hand arithmetic over reach, radius and step-in got this wrong**, because the blade's height matters as much as its reach: the first active ticks pass above a body and the last ones pass through it.

**The reference encounter**, from `combat-probe script` and `combat-probe moments`: 4,500 ticks (37.5 s) of `arena-exchange` on flat ground.

| counter | player | adversary |
|---|---|---|
| swings | 8 | 14 |
| hits landed | 6 | 6 |
| whiffs | 2 | 5 |
| dodges | 7 | 0 |
| staggers taken | 6 | 5 |
| defeats | 0 | 1 |
| lowest health | 6 | 0 |

One reset, 693 separations, 160 hit queries, worst sweep 4 substeps of a bound of 16, 91 multi-hit contacts suppressed, **zero events dropped**, and a worst body overlap after separation of `0.0000` world units. The player wins, and with six health left out of ninety-six — a reference fight that is a fight. All eleven named moments occur, the earliest at tick 1 and the last, the defeat, at tick 3,054.

**Partition equivalence**, from `combat-probe partition`: 20 s of wall time delivered at 30, 60 and 144 frames a second produced **2,399 ticks and the identical trace `0x0807fcad37689f9f`** in all three, with no capped frames and no dropped ticks. 144 does not divide a second exactly in nanoseconds, so the agreement is the clock's carried remainder working rather than an accident of the numbers.

**Cost**, release build on the audited Windows 11 / Intel Iris Xe host, as observations on that host:

| measure | value |
|---|---|
| combat tick, mean over 12,000 ticks after a warm-up | `5.1`–`8.9` µs across four runs |
| implied cost of a second of play at 120 Hz | `0.61`–`1.07` ms |
| combat tick, worst | `0.59`–`0.65` ms |
| ground queries per tick | `14.96` |
| one hit query at 16 substeps | `0.168` µs |
| weapon compile, median of 64 | `95.2` µs (min `84.1`, max `223.3`) |

The worst-case tick is two orders of magnitude above the mean and does not move with the work, so it is desktop scheduling rather than a tick: the figure to read is the mean, and a second of simulation costs about a millisecond of one core.

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
